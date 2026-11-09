//! GTFS tables as read from a feed, with string ids and raw values. How they
//! become a routable timetable (calendars for a date, missing times,
//! patterns) is decided by the transit layer, following R5.

use super::csv::{parse, Table};
use super::zip::{Archive, ZipError};
use std::fmt;

#[derive(Debug)]
pub enum GtfsError {
    Zip(ZipError),
    Missing(&'static str),
    Field {
        file: &'static str,
        row: usize,
        column: &'static str,
        value: String,
    },
}

impl fmt::Display for GtfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GtfsError::Zip(e) => write!(f, "{e}"),
            GtfsError::Missing(file) => write!(f, "GTFS feed has no {file}"),
            GtfsError::Field {
                file,
                row,
                column,
                value,
            } => {
                write!(f, "{file} row {row}: invalid {column} '{value}'")
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
        let table = |name: &'static str| -> Result<Option<Table>, GtfsError> {
            match a.find(name) {
                Some(e) => Ok(Some(parse(&a.read(e)?))),
                None => Ok(None),
            }
        };
        let required = |name: &'static str| -> Result<Table, GtfsError> {
            table(name)?.ok_or(GtfsError::Missing(name))
        };
        let mut feed = Feed::default();

        let t = required("stops.txt")?;
        let (id, lat, lon, lt, ps) = (
            t.col("stop_id"),
            t.col("stop_lat"),
            t.col("stop_lon"),
            t.col("location_type"),
            t.col("parent_station"),
        );
        for (i, r) in t.rows.iter().enumerate() {
            let num = |c, column| -> Result<f64, GtfsError> {
                let v = t.get(r, c);
                if v.is_empty() {
                    return Ok(f64::NAN); // stations without coordinates are allowed
                }
                v.parse().map_err(|_| GtfsError::Field {
                    file: "stops.txt",
                    row: i + 2,
                    column,
                    value: v.into(),
                })
            };
            feed.stops.push(Stop {
                stop_id: t.get(r, id).to_string(),
                lat: num(lat, "stop_lat")?,
                lon: num(lon, "stop_lon")?,
                location_type: t.get(r, lt).parse().unwrap_or(0),
                parent_station: t.get(r, ps).to_string(),
            });
        }

        let t = required("routes.txt")?;
        let (id, ty) = (t.col("route_id"), t.col("route_type"));
        for (i, r) in t.rows.iter().enumerate() {
            let v = t.get(r, ty);
            feed.routes.push(Route {
                route_id: t.get(r, id).to_string(),
                route_type: v.parse().map_err(|_| GtfsError::Field {
                    file: "routes.txt",
                    row: i + 2,
                    column: "route_type",
                    value: v.into(),
                })?,
            });
        }

        let t = required("trips.txt")?;
        let (tid, rid, sid, dir) = (
            t.col("trip_id"),
            t.col("route_id"),
            t.col("service_id"),
            t.col("direction_id"),
        );
        for r in &t.rows {
            feed.trips.push(Trip {
                trip_id: t.get(r, tid).to_string(),
                route_id: t.get(r, rid).to_string(),
                service_id: t.get(r, sid).to_string(),
                direction_id: t.get(r, dir).parse().ok(),
            });
        }

        let t = required("stop_times.txt")?;
        let (tid, arr, dep, sid, seq, pu, dof) = (
            t.col("trip_id"),
            t.col("arrival_time"),
            t.col("departure_time"),
            t.col("stop_id"),
            t.col("stop_sequence"),
            t.col("pickup_type"),
            t.col("drop_off_type"),
        );
        feed.stop_times.reserve(t.rows.len());
        for (i, r) in t.rows.iter().enumerate() {
            let time = |c, column| {
                let v = t.get(r, c);
                parse_time(v).map_err(|_| GtfsError::Field {
                    file: "stop_times.txt",
                    row: i + 2,
                    column,
                    value: v.into(),
                })
            };
            let v = t.get(r, seq);
            feed.stop_times.push(StopTime {
                trip_id: t.get(r, tid).to_string(),
                arrival: time(arr, "arrival_time")?,
                departure: time(dep, "departure_time")?,
                stop_id: t.get(r, sid).to_string(),
                stop_sequence: v.parse().map_err(|_| GtfsError::Field {
                    file: "stop_times.txt",
                    row: i + 2,
                    column: "stop_sequence",
                    value: v.into(),
                })?,
                pickup_type: t.get(r, pu).parse().unwrap_or(0),
                drop_off_type: t.get(r, dof).parse().unwrap_or(0),
            });
        }

        if let Some(t) = table("calendar.txt")? {
            let day_cols: Vec<_> = [
                "monday",
                "tuesday",
                "wednesday",
                "thursday",
                "friday",
                "saturday",
                "sunday",
            ]
            .iter()
            .map(|d| t.col(d))
            .collect();
            let (sid, sd, ed) = (t.col("service_id"), t.col("start_date"), t.col("end_date"));
            for (i, r) in t.rows.iter().enumerate() {
                let date = |c, column| {
                    let v = t.get(r, c);
                    v.parse::<u32>().map_err(|_| GtfsError::Field {
                        file: "calendar.txt",
                        row: i + 2,
                        column,
                        value: v.into(),
                    })
                };
                let mut days = [false; 7];
                for (d, c) in days.iter_mut().zip(&day_cols) {
                    *d = t.get(r, *c) == "1";
                }
                feed.calendars.push(Calendar {
                    service_id: t.get(r, sid).to_string(),
                    days,
                    start_date: date(sd, "start_date")?,
                    end_date: date(ed, "end_date")?,
                });
            }
        }

        if let Some(t) = table("calendar_dates.txt")? {
            let (sid, d, et) = (t.col("service_id"), t.col("date"), t.col("exception_type"));
            for (i, r) in t.rows.iter().enumerate() {
                let v = t.get(r, d);
                feed.calendar_dates.push(CalendarDate {
                    service_id: t.get(r, sid).to_string(),
                    date: v.parse().map_err(|_| GtfsError::Field {
                        file: "calendar_dates.txt",
                        row: i + 2,
                        column: "date",
                        value: v.into(),
                    })?,
                    exception_type: t.get(r, et).parse().unwrap_or(0),
                });
            }
        }

        if let Some(t) = table("frequencies.txt")? {
            let (tid, st, et, hw, ex) = (
                t.col("trip_id"),
                t.col("start_time"),
                t.col("end_time"),
                t.col("headway_secs"),
                t.col("exact_times"),
            );
            for (i, r) in t.rows.iter().enumerate() {
                let bad = |column: &'static str, v: &str| GtfsError::Field {
                    file: "frequencies.txt",
                    row: i + 2,
                    column,
                    value: v.into(),
                };
                let time = |c, column| -> Result<i32, GtfsError> {
                    let v = t.get(r, c);
                    parse_time(v).ok().flatten().ok_or_else(|| bad(column, v))
                };
                let v = t.get(r, hw);
                feed.frequencies.push(Frequency {
                    trip_id: t.get(r, tid).to_string(),
                    start_time: time(st, "start_time")?,
                    end_time: time(et, "end_time")?,
                    headway_secs: v.parse().map_err(|_| bad("headway_secs", v))?,
                    exact_times: t.get(r, ex) == "1",
                });
            }
        }
        Ok(feed)
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
}
