//! GTFS tables as read from a feed, with string ids and raw values. How they
//! become a routable timetable (calendars for a date, missing times,
//! patterns) is decided by the transit layer, following R5.

use super::csv::{for_each_row, CsvError, Header, ReadError, Row};
use super::zip::{Archive, ZipError};
use std::fmt;

#[derive(Debug)]
pub enum GtfsError {
    Zip(ZipError),
    Missing(&'static str),
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
}

impl fmt::Display for GtfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GtfsError::Zip(e) => write!(f, "{e}"),
            GtfsError::Missing(file) => write!(f, "GTFS feed has no {file}"),
            GtfsError::Csv { file, error } => write!(f, "{file} {error}"),
            GtfsError::Field {
                file,
                line,
                column,
                value,
            } => {
                write!(f, "{file} line {line}: invalid {column} '{value}'")
            }
        }
    }
}

impl std::error::Error for GtfsError {}

impl From<ZipError> for GtfsError {
    fn from(e: ZipError) -> Self {
        GtfsError::Zip(e)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stop {
    pub stop_id: String,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trip {
    pub trip_id: String,
    pub route_id: String,
    pub service_id: String,
    pub direction_id: Option<u8>,
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
    /// Read a feed from a GTFS zip file.
    pub fn read_zip(path: impl AsRef<std::path::Path>) -> Result<Feed, GtfsError> {
        Self::from_archive(&Archive::open(path)?)
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

        table(a, "feed_info.txt", false, |h, _, r| {
            let id = r.get(h.col("feed_id"));
            if feed.feed_id.is_none() && !id.is_empty() {
                feed.feed_id = Some(id.to_string());
            }
            Ok(())
        })?;

        table(a, "stops.txt", true, |h, n, r| {
            let num = |c, column| -> Result<f64, GtfsError> {
                let v = r.get(h.col(c));
                if v.is_empty() {
                    return Ok(f64::NAN); // stations without coordinates are allowed
                }
                v.parse().map_err(|_| bad("stops.txt", n, column, v))
            };
            feed.stops.push(Stop {
                stop_id: r.get(h.col("stop_id")).to_string(),
                lat: num("stop_lat", "stop_lat")?,
                lon: num("stop_lon", "stop_lon")?,
                location_type: r.get(h.col("location_type")).parse().unwrap_or(0),
                parent_station: r.get(h.col("parent_station")).to_string(),
            });
            Ok(())
        })?;

        table(a, "routes.txt", true, |h, n, r| {
            let v = r.get(h.col("route_type"));
            feed.routes.push(Route {
                route_id: r.get(h.col("route_id")).to_string(),
                route_type: v
                    .parse()
                    .map_err(|_| bad("routes.txt", n, "route_type", v))?,
            });
            Ok(())
        })?;

        table(a, "trips.txt", true, |h, _, r| {
            feed.trips.push(Trip {
                trip_id: r.get(h.col("trip_id")).to_string(),
                route_id: r.get(h.col("route_id")).to_string(),
                service_id: r.get(h.col("service_id")).to_string(),
                direction_id: r.get(h.col("direction_id")).parse().ok(),
            });
            Ok(())
        })?;

        table(a, "stop_times.txt", true, |h, n, r| {
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
        table(a, "calendar.txt", false, |h, n, r| {
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
        })?;

        table(a, "calendar_dates.txt", false, |h, n, r| {
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

        table(a, "frequencies.txt", false, |h, n, r| {
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
        })?;
        Ok(feed)
    }
}

/// Stream the records of one file of the archive into `f`.
fn table(
    a: &Archive,
    name: &'static str,
    required: bool,
    f: impl FnMut(&Header, usize, &Row) -> Result<(), GtfsError>,
) -> Result<(), GtfsError> {
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
}
