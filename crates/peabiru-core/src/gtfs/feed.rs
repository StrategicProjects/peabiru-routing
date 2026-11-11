//! GTFS tables as read from a feed, with string ids and raw values. How they
//! become a routable timetable (calendars for a date, missing times,
//! patterns) is decided by the transit layer, following R5.

use super::csv::{for_each_row, CsvError, Header, ReadError, Row};
use super::zip::{Archive, Limits, ZipError};
use std::collections::{BTreeMap, HashMap};
use std::fmt;

#[derive(Debug)]
pub enum GtfsError {
    Zip(ZipError),
    /// A required table is absent.
    Missing(&'static str),
    /// A table lacks a column that every record needs.
    MissingColumn {
        file: &'static str,
        column: &'static str,
    },
    Csv {
        file: &'static str,
        error: CsvError,
    },
    Field {
        file: &'static str,
        /// Line of the file on which the record starts.
        line: usize,
        column: &'static str,
        value: String,
    },
    /// Why gtfs-lib fails loading the feed (see `validate`).
    GtfsLibFails(String),
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
            GtfsError::Missing(file) => write!(f, "GTFS feed has no {file}"),
            GtfsError::MissingColumn { file, column } => {
                write!(f, "{file}: missing required column {column}")
            }
            GtfsError::Csv { file, error } => write!(f, "{file} {error}"),
            GtfsError::Field {
                file,
                line,
                column,
                value,
            } => {
                write!(f, "{file} line {line}: invalid {column} '{value}'")
            }
            GtfsError::GtfsLibFails(why) => f.write_str(why),
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
    pub lat: f64,
    pub lon: f64,
    /// 0 stop/platform, 1 station, 2 entrance, 3 generic node, 4 boarding area.
    pub location_type: u8,
    pub parent_station: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub route_id: String,
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
    pub route_id: String,
    pub service_id: String,
    pub direction_id: Option<u8>,
    /// Empty when blank.
    pub shape_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopTime {
    pub trip_id: String,
    /// Seconds after midnight of the service day; `None` when blank.
    pub arrival: Option<i32>,
    pub departure: Option<i32>,
    pub stop_id: String,
    pub stop_sequence: i32,
    /// 0 regular, 1 none, 2 phone, 3 coordinate with driver.
    pub pickup_type: u8,
    pub drop_off_type: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    pub service_id: String,
    /// Monday first.
    pub days: [bool; 7],
    /// `yyyymmdd`.
    pub start_date: u32,
    pub end_date: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarDate {
    pub service_id: String,
    pub date: u32,
    /// 1 added, 2 removed.
    pub exception_type: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frequency {
    pub trip_id: String,
    pub start_time: i32,
    pub end_time: i32,
    pub headway_secs: i32,
    pub exact_times: bool,
}

/// The tables of one feed.
#[derive(Debug, Clone, Default)]
pub struct Feed {
    /// `feed_info.feed_id`, when present.
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
}

/// GTFS time `H:MM:SS` (hours may exceed 23) to seconds; `None` when blank.
pub(crate) fn parse_time(s: &str) -> Result<Option<i32>, ()> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let mut parts = s.split(':');
    let mut next = || -> Result<i32, ()> {
        let p = parts.next().ok_or(())?;
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) || p.len() > 3 {
            return Err(());
        }
        p.parse().map_err(|_| ())
    };
    let (h, m, sec) = (next()?, next()?, next()?);
    if parts.next().is_some() || m > 59 || sec > 59 {
        return Err(());
    }
    Ok(Some(h * 3600 + m * 60 + sec))
}

impl Feed {
    /// Read a feed from the bytes of a GTFS zip file, with bounds on how
    /// far the archive may expand, also saying how the archive is laid out
    /// (for the warnings about feeds that r5r refuses). Errors name the
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

    pub fn from_archive(a: &Archive) -> Result<Feed, GtfsError> {
        let mut feed = Feed::default();
        let bad =
            |file: &'static str, line: usize, column: &'static str, value: &str| GtfsError::Field {
                file,
                line,
                column,
                value: value.into(),
            };

        table(a, "agency.txt", false, &[], |h, _, r| {
            feed.agencies.push((
                r.get(h.col("agency_id")).to_string(),
                r.get(h.col("agency_name")).to_string(),
            ));
            Ok(())
        })?;

        table(a, "feed_info.txt", false, &[], |h, _, r| {
            let id = r.get(h.col("feed_id"));
            if feed.feed_id.is_none() && !id.is_empty() {
                feed.feed_id = Some(id.to_string());
            }
            Ok(())
        })?;

        table(a, "stops.txt", true, &[], |h, n, r| {
            let num = |c, column| -> Result<f64, GtfsError> {
                let v = r.get(h.col(c));
                if v.is_empty() {
                    return Ok(f64::NAN); // stations without coordinates are allowed
                }
                v.parse().map_err(|_| bad("stops.txt", n, column, v))
            };
            feed.stops.push(Stop {
                stop_id: r.get(h.col("stop_id")).to_string(),
                stop_name: r.get(h.col("stop_name")).to_string(),
                lat: num("stop_lat", "stop_lat")?,
                lon: num("stop_lon", "stop_lon")?,
                location_type: r.get(h.col("location_type")).parse().unwrap_or(0),
                parent_station: r.get(h.col("parent_station")).to_string(),
            });
            Ok(())
        })?;

        table(a, "routes.txt", true, &["route_type"], |h, n, r| {
            let v = r.get(h.col("route_type"));
            feed.routes.push(Route {
                route_id: r.get(h.col("route_id")).to_string(),
                route_type: v
                    .parse()
                    .map_err(|_| bad("routes.txt", n, "route_type", v))?,
                agency_id: r.get(h.col("agency_id")).to_string(),
                short_name: r.get(h.col("route_short_name")).to_string(),
                long_name: r.get(h.col("route_long_name")).to_string(),
            });
            Ok(())
        })?;

        table(a, "trips.txt", true, &[], |h, _, r| {
            feed.trips.push(Trip {
                trip_id: r.get(h.col("trip_id")).to_string(),
                route_id: r.get(h.col("route_id")).to_string(),
                service_id: r.get(h.col("service_id")).to_string(),
                direction_id: r.get(h.col("direction_id")).parse().ok(),
                shape_id: r.get(h.col("shape_id")).to_string(),
            });
            Ok(())
        })?;

        table(a, "stop_times.txt", true, &["stop_sequence"], |h, n, r| {
            let time = |c, column| {
                let v = r.get(h.col(c));
                parse_time(v).map_err(|_| bad("stop_times.txt", n, column, v))
            };
            let seq = r.get(h.col("stop_sequence"));
            feed.stop_times.push(StopTime {
                trip_id: r.get(h.col("trip_id")).to_string(),
                arrival: time("arrival_time", "arrival_time")?,
                departure: time("departure_time", "departure_time")?,
                stop_id: r.get(h.col("stop_id")).to_string(),
                stop_sequence: seq
                    .parse()
                    .map_err(|_| bad("stop_times.txt", n, "stop_sequence", seq))?,
                pickup_type: r.get(h.col("pickup_type")).parse().unwrap_or(0),
                drop_off_type: r.get(h.col("drop_off_type")).parse().unwrap_or(0),
            });
            Ok(())
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
        table(
            a,
            "calendar.txt",
            false,
            &["start_date", "end_date"],
            |h, n, r| {
                let date = |c, column| {
                    let v = r.get(h.col(c));
                    v.parse::<u32>()
                        .map_err(|_| bad("calendar.txt", n, column, v))
                };
                let mut days = [false; 7];
                for (d, name) in days.iter_mut().zip(DAYS) {
                    *d = r.get(h.col(name)) == "1";
                }
                feed.calendars.push(Calendar {
                    service_id: r.get(h.col("service_id")).to_string(),
                    days,
                    start_date: date("start_date", "start_date")?,
                    end_date: date("end_date", "end_date")?,
                });
                Ok(())
            },
        )?;

        table(a, "calendar_dates.txt", false, &["date"], |h, n, r| {
            let v = r.get(h.col("date"));
            feed.calendar_dates.push(CalendarDate {
                service_id: r.get(h.col("service_id")).to_string(),
                date: v
                    .parse()
                    .map_err(|_| bad("calendar_dates.txt", n, "date", v))?,
                exception_type: r.get(h.col("exception_type")).parse().unwrap_or(0),
            });
            Ok(())
        })?;

        table(
            a,
            "frequencies.txt",
            false,
            &["start_time", "end_time", "headway_secs"],
            |h, n, r| {
                let time = |c, column| -> Result<i32, GtfsError> {
                    let v = r.get(h.col(c));
                    parse_time(v)
                        .ok()
                        .flatten()
                        .ok_or_else(|| bad("frequencies.txt", n, column, v))
                };
                let hw = r.get(h.col("headway_secs"));
                feed.frequencies.push(Frequency {
                    trip_id: r.get(h.col("trip_id")).to_string(),
                    start_time: time("start_time", "start_time")?,
                    end_time: time("end_time", "end_time")?,
                    headway_secs: hw
                        .parse()
                        .map_err(|_| bad("frequencies.txt", n, "headway_secs", hw))?,
                    exact_times: r.get(h.col("exact_times")) == "1",
                });
                Ok(())
            },
        )?;

        // Shapes are only drawn (r5r's transit_network_to_sf()), never
        // routed on, so values gtfs-lib cannot parse are read as it reads
        // them (NaN, a missing sequence) rather than refused.
        table(a, "shapes.txt", false, &[], |h, _, r| {
            let id = r.get(h.col("shape_id"));
            if id.is_empty() {
                return Ok(());
            }
            let num = |c| super::jparse::parse_double(r.get(h.col(c))).unwrap_or(f64::NAN);
            let seq = super::jparse::parse_int(r.get(h.col("shape_pt_sequence")))
                .unwrap_or(super::jparse::INT_MISSING);
            let point = (num("shape_pt_lon"), num("shape_pt_lat"));
            match feed.shapes.get_mut(id) {
                Some(s) => {
                    s.insert(seq, point);
                }
                None => {
                    feed.shapes
                        .insert(id.to_string(), BTreeMap::from([(seq, point)]));
                }
            }
            Ok(())
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
/// (and so r5r) refuse a feed that peabiru reads.
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

/// Stream the records of one file of the archive into `f`. `columns` are
/// the columns whose empty value is an error; when the table has records,
/// their absence from the header is reported once as a missing column.
fn table(
    a: &Archive,
    name: &'static str,
    required: bool,
    columns: &[&'static str],
    mut f: impl FnMut(&Header, usize, &Row) -> Result<(), GtfsError>,
) -> Result<(), GtfsError> {
    let mut checked = false;
    let f = |h: &Header, line: usize, r: &Row| {
        if !checked {
            if let Some(&column) = columns.iter().find(|c| h.col(c).is_none()) {
                return Err(GtfsError::MissingColumn { file: name, column });
            }
            checked = true;
        }
        f(h, line, r)
    };
    match a.find(name) {
        Some(e) => match for_each_row(&a.read(e)?, f) {
            Ok(_) => Ok(()),
            Err(ReadError::Csv(error)) => Err(GtfsError::Csv { file: name, error }),
            Err(ReadError::Callback(e)) => Err(e),
        },
        None if required => Err(GtfsError::Missing(name)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtfs::zip::tests::make_zip;

    #[test]
    fn times() {
        assert_eq!(parse_time("08:05:09"), Ok(Some(8 * 3600 + 5 * 60 + 9)));
        assert_eq!(parse_time(" 7:00:00"), Ok(Some(7 * 3600)));
        assert_eq!(parse_time("25:30:00"), Ok(Some(25 * 3600 + 1800)));
        assert_eq!(parse_time(""), Ok(None));
        assert!(parse_time("8:60:00").is_err());
        assert!(parse_time("8:00").is_err());
        assert!(parse_time("x:00:00").is_err());
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
        assert!(!f.frequencies[0].exact_times);
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

    #[test]
    fn missing_required_column_is_reported_as_such() {
        let e = feed_with(&[("routes.txt", b"route_id\nR\n")]).unwrap_err();
        assert_eq!(
            e.to_string(),
            "routes.txt: missing required column route_type"
        );
        let e =
            feed_with(&[("calendar_dates.txt", b"service_id,exception_type\nS,1\n")]).unwrap_err();
        assert_eq!(
            e.to_string(),
            "calendar_dates.txt: missing required column date"
        );
        // a table without records needs no columns
        assert!(feed_with(&[("calendar_dates.txt", b"service_id\n")]).is_ok());
    }

    #[test]
    fn field_errors_give_the_file_line() {
        let e = feed_with(&[(
            "routes.txt",
            b"route_id,route_long_name,route_type\nR,\"two\nlines\",3\n\nQ,x,bus\n",
        )])
        .unwrap_err();
        assert_eq!(e.to_string(), "routes.txt line 5: invalid route_type 'bus'");
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
