//! The errors conveyal gtfs-lib (as bundled in R5 7.6) records while it
//! loads a GTFS feed: `GTFSFeed.loadFromFile` with the table loaders of
//! `com.conveyal.gtfs.model` and `PostLoadValidator`. r5r 2.5.0 writes
//! them to `gtfs_errors.csv` and refuses to build a network when any has
//! priority `HIGH`; `get_gtfs_errors()` reads them back.
//!
//! The loaders are ported field by field, in gtfs-lib's order, with its
//! quirks: a value missing from a required column is an `EmptyFieldError`
//! on every row (javacsv reads an absent column as empty, so gtfs-lib's
//! `MissingColumnError` never happens); a reference is checked the first
//! time a value is seen in a column, across the whole feed; a column read
//! twice (`from_stop_id`, `trip_id`, ...) reports an empty value twice;
//! `line` counts records, the header being line 1.
//!
//! Where gtfs-lib throws instead of recording an error, and r5r's
//! `build_network()` fails without writing `gtfs_errors.csv`, this returns
//! an error saying so ([`Outcome::Fails`]).

use super::csv::{for_each_row, Header, ReadError, Row};
use super::jparse::{parse_double, parse_int, INT_MISSING};
use super::zip::{Archive, Limits};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// gtfs-lib's `Priority` of an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GtfsErrorPriority {
    /// Makes r5r refuse to build the network.
    High,
    /// `MEDIUM`.
    Medium,
    /// `LOW`.
    Low,
    /// `UNKNOWN` (gtfs-lib's `GeneralError`).
    Unknown,
}

impl GtfsErrorPriority {
    /// The name r5r writes: `"HIGH"`, `"MEDIUM"`, `"LOW"` or `"UNKNOWN"`.
    pub fn as_str(self) -> &'static str {
        match self {
            GtfsErrorPriority::High => "HIGH",
            GtfsErrorPriority::Medium => "MEDIUM",
            GtfsErrorPriority::Low => "LOW",
            GtfsErrorPriority::Unknown => "UNKNOWN",
        }
    }
}

/// One row of r5r's `gtfs_errors.csv`: an error gtfs-lib recorded while
/// loading a feed. Made by [`gtfs_errors`](crate::gtfs_errors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GtfsFeedError {
    file: Option<String>,
    line: i32,
    error_type: &'static str,
    field: Option<String>,
    id: Option<String>,
    priority: GtfsErrorPriority,
}

impl GtfsFeedError {
    /// `file`: the table, without `.txt` (such as `"stops"`).
    pub fn file(&self) -> Option<&str> {
        self.file.as_deref()
    }

    /// `line`: the record, counting the header as line 1 (a quoted value
    /// with a line break is still one record); 0 for errors about a whole
    /// table or feed.
    pub fn line(&self) -> i32 {
        self.line
    }

    /// `type`: gtfs-lib's error class, such as `"DuplicateKeyError"`.
    pub fn error_type(&self) -> &'static str {
        self.error_type
    }

    /// `field`: the column, when the error has one.
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }

    /// `id`: the key or entity concerned, for the errors that carry one
    /// (duplicate keys, suspect stop locations).
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// `priority`.
    pub fn priority(&self) -> GtfsErrorPriority {
        self.priority
    }
}

/// The result of validating one feed.
pub(crate) enum Outcome {
    /// gtfs-lib loads the feed, recording these errors.
    Errors(Vec<GtfsFeedError>),
    /// gtfs-lib throws while loading the feed (or peabiru cannot tell what
    /// it records): why.
    Fails(String),
}

/// Columns of a `stop_times.txt` row that make it a GTFS-Flex stop time.
const FLEX_FIELDS: [&str; 6] = [
    "start_pickup_drop_off_window",
    "end_pickup_drop_off_window",
    "location_id",
    "location_group_id",
    "pickup_booking_rule_id",
    "drop_off_booking_rule_id",
];

/// A stop as `PostLoadValidator` sees it.
struct StopRec {
    line: i64,
    location_type: i32,
    parent_station: Option<String>,
    lat: f64,
    lon: f64,
    /// Whether `stop_name` has a value.
    named: bool,
}

/// A trip as `GTFSFeed.findPatterns` sees it.
struct TripRec {
    line: i64,
    route: Option<String>,
    shape: Option<String>,
}

/// A stop time as `GTFSFeed.findPatterns` sees it: its row, its stop (an
/// index into `Feed.stop_names`) and its pickup and drop-off types.
#[derive(Clone, Copy)]
struct StopTimeRec {
    line: i64,
    stop: Option<u32>,
    pickup: i32,
    drop_off: i32,
}

/// The feed being loaded: what gtfs-lib keeps in `GTFSFeed`.
#[derive(Default)]
struct Feed {
    errors: Vec<GtfsFeedError>,
    /// `GTFSFeed.transitIds`: `column:value` already checked.
    transit_ids: HashSet<String>,
    feed_info_seen: bool,
    agencies: BTreeMap<String, ()>,
    /// Service ids with a calendar, and the dates of their exceptions
    /// (`None` for an empty date).
    calendars: HashSet<Option<String>>,
    services: HashMap<Option<String>, HashSet<Option<u32>>>,
    fares: HashMap<Option<String>, bool>,
    routes: HashSet<String>,
    stops: BTreeMap<String, StopRec>,
    trips: HashMap<String, TripRec>,
    /// The sequence numbers of each shape's points.
    shapes: HashMap<String, BTreeSet<i32>>,
    flex_groups: HashSet<Option<String>>,
    /// Trips with a GTFS-Flex stop time.
    flex_trips: BTreeMap<Option<String>, ()>,
    /// Stop times per trip by `stop_sequence`, a repeated sequence keeping
    /// the last row.
    stop_times: HashMap<Option<String>, BTreeMap<i32, StopTimeRec>>,
    /// The stop ids of `stop_times.txt`, interned.
    stop_index: HashMap<String, u32>,
    stop_names: Vec<String>,
    fail: Option<String>,
}

impl Feed {
    fn intern_stop(&mut self, id: &str) -> u32 {
        if let Some(&i) = self.stop_index.get(id) {
            return i;
        }
        let i = self.stop_names.len() as u32;
        self.stop_names.push(id.to_string());
        self.stop_index.insert(id.to_string(), i);
        i
    }
}

/// A failure: what gtfs-lib throws.
fn fails(f: &mut Feed, why: String) {
    if f.fail.is_none() {
        f.fail = Some(why);
    }
}

impl Feed {
    fn push(
        &mut self,
        file: &str,
        line: i64,
        error_type: &'static str,
        field: Option<&str>,
        id: Option<&str>,
        priority: GtfsErrorPriority,
    ) {
        self.errors.push(GtfsFeedError {
            file: Some(file.to_string()),
            // GTFSError.line is a long; r5r stores it as an int
            line: line as i32,
            error_type,
            field: field.map(str::to_string),
            id: id.map(str::to_string),
            priority,
        });
    }
}

/// One table being read: gtfs-lib's `Entity.Loader` field getters.
struct Table<'a, 'r> {
    name: &'static str,
    row: i64,
    h: &'a Header,
    r: &'a Row<'r>,
}

impl Table<'_, '_> {
    /// `getFieldCheckRequired`: `None` when empty, an `EmptyFieldError`
    /// when also required.
    fn field(&self, f: &mut Feed, col: &str, required: bool) -> Option<String> {
        let v = self.r.get(self.h.col(col));
        if v.is_empty() {
            if required {
                f.push(
                    self.name,
                    self.row,
                    "EmptyFieldError",
                    Some(col),
                    None,
                    GtfsErrorPriority::Medium,
                );
            }
            None
        } else {
            Some(v.to_string())
        }
    }

    fn range(&self, f: &mut Feed, min: f64, max: f64, actual: f64) {
        if actual < min || actual > max {
            f.push(
                self.name,
                self.row,
                "RangeError",
                None,
                None,
                GtfsErrorPriority::Low,
            );
        }
    }

    fn int(
        &self,
        f: &mut Feed,
        col: &str,
        required: bool,
        min: i32,
        max: i32,
        default: i32,
    ) -> i32 {
        let Some(s) = self.field(f, col, required) else {
            return default;
        };
        match parse_int(&s) {
            Some(v) => {
                self.range(f, f64::from(min), f64::from(max), f64::from(v));
                v
            }
            None => {
                f.push(
                    self.name,
                    self.row,
                    "NumberParseError",
                    Some(col),
                    None,
                    GtfsErrorPriority::High,
                );
                INT_MISSING
            }
        }
    }

    fn double(&self, f: &mut Feed, col: &str, required: bool, min: f64, max: f64) -> f64 {
        let Some(s) = self.field(f, col, required) else {
            return f64::NAN;
        };
        match parse_double(&s) {
            Some(v) => {
                self.range(f, min, max, v);
                v
            }
            None => {
                f.push(
                    self.name,
                    self.row,
                    "NumberParseError",
                    Some(col),
                    None,
                    GtfsErrorPriority::High,
                );
                f64::NAN
            }
        }
    }

    /// `getTimeField`: `H:M:S`, split as Java's `String.split(":")` does
    /// (trailing empty parts dropped).
    fn time(&self, f: &mut Feed, col: &str, required: bool) {
        let Some(s) = self.field(f, col, required) else {
            return;
        };
        let mut parts: Vec<&str> = s.split(':').collect();
        while parts.len() > 1 && parts.last() == Some(&"") {
            parts.pop();
        }
        let parsed: Option<Vec<i32>> = (parts.len() == 3)
            .then(|| parts.iter().map(|p| parse_int(p)).collect())
            .flatten();
        match parsed {
            Some(v) => {
                self.range(f, 0.0, 72.0, f64::from(v[0]));
                self.range(f, 0.0, 59.0, f64::from(v[1]));
                self.range(f, 0.0, 59.0, f64::from(v[2]));
            }
            None => f.push(
                self.name,
                self.row,
                "TimeParseError",
                Some(col),
                None,
                GtfsErrorPriority::Medium,
            ),
        }
    }

    /// `getDateField`: `LocalDate.parse(s, BASIC_ISO_DATE)`. A value it
    /// cannot parse throws `DateTimeParseException`, which gtfs-lib does
    /// not catch (it catches `IllegalArgumentException`): the load fails.
    fn date(&self, f: &mut Feed, col: &str, required: bool) -> Option<u32> {
        let s = self.field(f, col, required)?;
        match basic_iso_date(&s) {
            Some(d) => {
                self.range(f, 2000.0, 2100.0, f64::from(d / 10000));
                Some(d)
            }
            None => {
                fails(
                    f,
                    format!(
                        "{}.txt line {}: {col} '{s}' is not a date written YYYYMMDD, and \
                         gtfs-lib fails reading it (DateTimeParseException)",
                        self.name, self.row
                    ),
                );
                None
            }
        }
    }

    fn url(&self, f: &mut Feed, col: &str, required: bool) {
        if let Some(s) = self.field(f, col, required) {
            if !java_url_ok(&s) {
                f.push(
                    self.name,
                    self.row,
                    "URLParseError",
                    Some(col),
                    None,
                    GtfsErrorPriority::Low,
                );
            }
        }
    }

    /// `getRefField`: the value, checked the first time it is seen in this
    /// column anywhere in the feed.
    fn reference(
        &self,
        f: &mut Feed,
        col: &str,
        required: bool,
        exists: impl Fn(&Feed, &str) -> bool,
    ) -> Option<String> {
        let s = self.field(f, col, required)?;
        let key = format!("{col}:{s}");
        if !f.transit_ids.contains(&key) {
            f.transit_ids.insert(key);
            if !exists(f, &s) {
                f.push(
                    self.name,
                    self.row,
                    "ReferentialIntegrityError",
                    Some(col),
                    None,
                    GtfsErrorPriority::High,
                );
            }
        }
        Some(s)
    }
}

/// Java's `LocalDate.parse(s, DateTimeFormatter.BASIC_ISO_DATE)`: eight
/// digits making a date, optionally followed by an offset (`Z`, `+hh`,
/// `+hhmm`, `+hhmmss`); the date as `yyyymmdd`.
fn basic_iso_date(s: &str) -> Option<u32> {
    if s.len() < 8 || !s.is_char_boundary(8) || !s.as_bytes()[..8].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let (date, offset) = s.split_at(8);
    let d: u32 = date.parse().ok()?;
    crate::transit::matrix::check_date(d).ok()?;
    let offset_ok = match offset.as_bytes() {
        [] | [b'Z'] => true,
        [b'+' | b'-', rest @ ..] => {
            let ok_len = matches!(rest.len(), 2 | 4 | 6);
            ok_len
                && rest.iter().all(u8::is_ascii_digit)
                && rest.chunks(2).enumerate().all(|(i, c)| {
                    let v = u32::from(c[0] - b'0') * 10 + u32::from(c[1] - b'0');
                    if i == 0 {
                        v <= 18
                    } else {
                        v <= 59
                    }
                })
        }
        _ => false,
    };
    offset_ok.then_some(d)
}

/// Whether `new java.net.URL(s)` accepts `s` (Java 21): a scheme with a
/// built-in handler and, for the hierarchical ones, a valid port and IPv6
/// literal; a `jar:` URL needs `!/`.
fn java_url_ok(s: &str) -> bool {
    let s = s.trim_matches(|c: char| c <= ' ');
    let s = if s.len() >= 4 && s[..4].eq_ignore_ascii_case("url:") {
        &s[4..]
    } else {
        s
    };
    let Some(colon) = s.find([':', '/']).filter(|&i| s.as_bytes()[i] == b':') else {
        return false;
    };
    let scheme = s[..colon].to_ascii_lowercase();
    let valid = scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'));
    if !valid {
        return false;
    }
    let rest = &s[colon + 1..];
    match scheme.as_str() {
        "http" | "https" | "ftp" | "file" | "jrt" | "jmod" => {
            let Some(after) = rest.strip_prefix("//") else {
                return true;
            };
            let end = after.find(['/', '?', '#']).unwrap_or(after.len());
            let authority = &after[..end];
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            let port = if let Some(v6) = host.strip_prefix('[') {
                let Some(close) = v6.find(']') else {
                    return false;
                };
                let inner = &v6[..close];
                if inner.is_empty()
                    || !inner
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
                {
                    return false;
                }
                match &v6[close + 1..] {
                    "" => None,
                    p => match p.strip_prefix(':') {
                        Some(p) => Some(p),
                        None => return false,
                    },
                }
            } else {
                host.rsplit_once(':').map(|(_, p)| p)
            };
            match port {
                None | Some("") => true,
                Some(p) => parse_int(p).is_some_and(|v| v >= -1),
            }
        }
        "jar" => rest.contains("!/"),
        "mailto" => true,
        _ => false,
    }
}

/// gtfs-lib's `Entity.Loader.loadTable`: the entry `name.txt` at the root,
/// else every entry whose name ends in it (each one a
/// `TableInSubdirectoryError`, the last one read); a missing required
/// table is a `MissingTableError` (also when found in a subfolder); an
/// empty file is two `EmptyTableError`s, a header alone one.
fn load_table(
    f: &mut Feed,
    a: &Archive,
    name: &'static str,
    required: bool,
    mut row_fn: impl FnMut(&mut Feed, &Table),
) -> Result<(), String> {
    let file = format!("{name}.txt");
    let mut entry = a.entries.iter().rev().find(|e| e.name == file);
    if entry.is_none() {
        for e in &a.entries {
            if e.name.ends_with(&file) {
                entry = Some(e);
                f.push(
                    name,
                    0,
                    "TableInSubdirectoryError",
                    None,
                    None,
                    GtfsErrorPriority::High,
                );
            }
        }
        if required {
            f.push(
                name,
                0,
                "MissingTableError",
                None,
                None,
                GtfsErrorPriority::Medium,
            );
        }
    }
    let Some(entry) = entry else {
        return Ok(());
    };
    let bytes = a.read(entry).map_err(|e| e.to_string())?;
    let mut row = 1i64;
    let header = for_each_row::<()>(&bytes, |h, _, r| {
        row += 1;
        let t = Table { name, row, h, r };
        row_fn(f, &t);
        Ok(())
    });
    match header {
        Ok(None) => f.push(
            name,
            0,
            "EmptyTableError",
            None,
            None,
            GtfsErrorPriority::Medium,
        ),
        Ok(Some(_)) => {}
        Err(ReadError::Csv(e)) => return Err(format!("{file} {e}")),
        Err(ReadError::Callback(())) => {}
    }
    if row < 2 {
        f.push(
            name,
            0,
            "EmptyTableError",
            None,
            None,
            GtfsErrorPriority::Medium,
        );
    }
    Ok(())
}

/// The errors gtfs-lib records loading the feed in `bytes` (see the module
/// documentation).
pub(crate) fn validate(bytes: &[u8]) -> Outcome {
    let a = match Archive::with_limits(bytes, Limits::default()) {
        Ok(a) => a,
        Err(e) => return Outcome::Fails(e.to_string()),
    };
    if a.entries
        .iter()
        .any(|e| e.name.ends_with("locations.geojson"))
    {
        return Outcome::Fails(
            "the feed has GTFS-Flex zones (locations.geojson), which peabiru does not read, so \
             the errors gtfs-lib finds in them cannot be listed"
                .to_string(),
        );
    }
    let mut f = Feed::default();
    match load_all(&mut f, &a) {
        Err(e) => Outcome::Fails(e),
        Ok(()) => match f.fail.take() {
            Some(why) => Outcome::Fails(why),
            None => Outcome::Errors(f.errors),
        },
    }
}

fn load_all(f: &mut Feed, a: &Archive) -> Result<(), String> {
    load_table(f, a, "feed_info", false, |f, t| {
        t.field(f, "feed_id", false);
        t.field(f, "feed_publisher_name", true);
        t.url(f, "feed_publisher_url", true);
        t.field(f, "feed_lang", true);
        t.date(f, "feed_start_date", false);
        t.date(f, "feed_end_date", false);
        t.field(f, "feed_version", false);
        if !f.feed_info_seen {
            f.feed_info_seen = true;
        } else {
            f.push(
                t.name,
                t.row,
                "GeneralError",
                None,
                None,
                GtfsErrorPriority::Unknown,
            );
        }
    })?;

    load_table(f, a, "agency", true, |f, t| {
        let id = t.field(f, "agency_id", false);
        t.field(f, "agency_name", true);
        t.url(f, "agency_url", true);
        t.field(f, "agency_lang", false);
        t.field(f, "agency_email", false);
        t.field(f, "agency_phone", false);
        t.field(f, "agency_timezone", true);
        t.url(f, "agency_fare_url", false);
        t.url(f, "agency_branding_url", false);
        let id = id.unwrap_or_else(|| "NONE".to_string());
        if f.agencies.insert(id.clone(), ()).is_some() {
            f.push(
                t.name,
                t.row,
                "DuplicateKeyError",
                Some("agency_id"),
                Some(&id),
                GtfsErrorPriority::Medium,
            );
        }
    })?;

    load_table(f, a, "calendar", false, |f, t| {
        let id = t.field(f, "service_id", true);
        f.services.entry(id.clone()).or_default();
        if f.calendars.contains(&id) {
            f.push(
                t.name,
                t.row,
                "DuplicateKeyError",
                Some("service_id"),
                id.as_deref(),
                GtfsErrorPriority::Medium,
            );
            return;
        }
        f.calendars.insert(id);
        for day in [
            "monday",
            "tuesday",
            "wednesday",
            "thursday",
            "friday",
            "saturday",
            "sunday",
        ] {
            t.int(f, day, true, 0, 1, 0);
        }
        t.int(f, "start_date", true, 18_500_101, 22_001_231, 0);
        t.int(f, "end_date", true, 18_500_101, 22_001_231, 0);
    })?;

    load_table(f, a, "calendar_dates", false, |f, t| {
        let id = t.field(f, "service_id", true);
        f.services.entry(id.clone()).or_default();
        let date = t.date(f, "date", true);
        if f.services[&id].contains(&date) {
            let Some(d) = date else {
                fails(
                    f,
                    format!(
                        "calendar_dates.txt line {}: a second empty date for service '{}' makes \
                     gtfs-lib fail (NullPointerException)",
                        t.row,
                        id.as_deref().unwrap_or("null")
                    ),
                );
                return;
            };
            let key = format!(
                "({},{:04}-{:02}-{:02})",
                id.as_deref().unwrap_or("null"),
                d / 10000,
                (d / 100) % 100,
                d % 100
            );
            f.push(
                t.name,
                t.row,
                "DuplicateKeyError",
                Some("(service_id, date)"),
                Some(&key),
                GtfsErrorPriority::Medium,
            );
        } else {
            f.services.get_mut(&id).expect("just added").insert(date);
            t.int(f, "exception_type", true, 1, 2, 0);
        }
    })?;
    if f.services.contains_key(&None) {
        fails(
            f,
            "a calendar or calendar_dates row without service_id makes gtfs-lib fail \
                  storing the services (NullPointerException)"
                .to_string(),
        );
    }

    load_table(f, a, "fare_attributes", false, |f, t| {
        let id = t.field(f, "fare_id", true);
        if f.fares.get(&id) == Some(&true) {
            f.push(
                t.name,
                t.row,
                "DuplicateKeyError",
                Some("fare_id"),
                id.as_deref(),
                GtfsErrorPriority::Medium,
            );
            return;
        }
        f.fares.insert(id, true);
        t.double(f, "price", true, 0.0, f64::from(i32::MAX));
        t.field(f, "currency_type", true);
        t.int(f, "payment_method", true, 0, 1, 0);
        t.int(f, "transfers", false, 0, 10, i32::MAX);
        t.int(f, "transfer_duration", false, 0, 24 * 60 * 60, 0);
    })?;

    load_table(f, a, "fare_rules", false, |f, t| {
        let id = t.field(f, "fare_id", true);
        if !f.fares.contains_key(&id) {
            f.push(
                t.name,
                t.row,
                "ReferentialIntegrityError",
                Some("fare_id"),
                None,
                GtfsErrorPriority::High,
            );
        }
        f.fares.entry(id).or_insert(false);
        for c in ["route_id", "origin_id", "destination_id", "contains_id"] {
            t.field(f, c, false);
        }
    })?;
    if f.fares.contains_key(&None) {
        fails(
            f,
            "a fare_attributes or fare_rules row without fare_id makes gtfs-lib fail \
                  storing the fares (NullPointerException)"
                .to_string(),
        );
    }

    load_table(f, a, "routes", true, |f, t| {
        let id = t.field(f, "route_id", true);
        let agency = t.reference(f, "agency_id", false, |f, s| f.agencies.contains_key(s));
        let found = agency.is_some_and(|s| f.agencies.contains_key(&s));
        if !found && f.agencies.is_empty() {
            f.push(
                "agency",
                0,
                "NoAgencyInFeedError",
                Some("agency_id"),
                None,
                GtfsErrorPriority::High,
            );
        }
        for c in ["route_short_name", "route_long_name", "route_desc"] {
            t.field(f, c, false);
        }
        t.int(f, "route_type", true, 0, 1500, 0);
        t.url(f, "route_url", false);
        t.field(f, "route_color", false);
        t.field(f, "route_text_color", false);
        t.url(f, "route_branding_url", false);
        match id {
            None => f.push(
                t.name,
                t.row,
                "MissingKeyError",
                Some("route_id"),
                None,
                GtfsErrorPriority::Medium,
            ),
            Some(id) => {
                if !f.routes.insert(id.clone()) {
                    f.push(
                        t.name,
                        t.row,
                        "DuplicateKeyError",
                        Some("route_id"),
                        Some(&id),
                        GtfsErrorPriority::Medium,
                    );
                }
            }
        }
    })?;

    load_table(f, a, "shapes", false, |f, t| {
        let id = t.field(f, "shape_id", true);
        t.double(f, "shape_pt_lat", true, -90.0, 90.0);
        t.double(f, "shape_pt_lon", true, -180.0, 180.0);
        let seq = t.int(f, "shape_pt_sequence", true, 0, i32::MAX, 0);
        t.double(f, "shape_dist_traveled", false, 0.0, f64::MAX);
        if let Some(id) = id {
            f.shapes.entry(id).or_default().insert(seq);
        }
    })?;

    load_table(f, a, "stops", true, |f, t| {
        let id = t.field(f, "stop_id", true);
        t.field(f, "stop_code", false);
        let named = t.field(f, "stop_name", true).is_some();
        t.field(f, "stop_desc", false);
        t.field(f, "zone_id", false);
        t.url(f, "stop_url", false);
        let location_type = t.int(f, "location_type", false, 0, 4, 0);
        let coords_required = location_type <= 2;
        let lat = t.double(f, "stop_lat", coords_required, -90.0, 90.0);
        let lon = t.double(f, "stop_lon", coords_required, -180.0, 180.0);
        let parent_station = t.field(f, "parent_station", location_type >= 2);
        t.field(f, "stop_timezone", false);
        t.field(f, "wheelchair_boarding", false);
        match id {
            None => f.push(
                t.name,
                t.row,
                "MissingKeyError",
                Some("stop_id"),
                None,
                GtfsErrorPriority::Medium,
            ),
            Some(id) => {
                let rec = StopRec {
                    line: t.row,
                    location_type,
                    parent_station,
                    lat,
                    lon,
                    named,
                };
                if f.stops.insert(id.clone(), rec).is_some() {
                    f.push(
                        t.name,
                        t.row,
                        "DuplicateKeyError",
                        Some("stop_id"),
                        Some(&id),
                        GtfsErrorPriority::Medium,
                    );
                }
            }
        }
    })?;

    load_table(f, a, "transfers", false, |f, t| {
        t.field(f, "from_stop_id", true);
        t.field(f, "to_stop_id", true);
        t.int(f, "transfer_type", true, 0, 5, 0);
        t.int(f, "min_transfer_time", false, 0, i32::MAX, 0);
        for c in ["from_route_id", "to_route_id", "from_trip_id", "to_trip_id"] {
            t.field(f, c, false);
        }
        t.reference(f, "from_stop_id", true, |f, s| f.stops.contains_key(s));
        t.reference(f, "to_stop_id", true, |f, s| f.stops.contains_key(s));
    })?;

    load_table(f, a, "trips", true, |f, t| {
        t.double(f, "safe_duration_factor", false, 1.0, 10.0);
        t.double(f, "safe_duration_offset", false, 0.0, 3600.0);
        let route = t.field(f, "route_id", true);
        t.field(f, "service_id", true);
        let id = t.field(f, "trip_id", true);
        for c in ["trip_headsign", "trip_short_name"] {
            t.field(f, c, false);
        }
        t.int(f, "direction_id", false, 0, 1, 0);
        t.field(f, "block_id", false);
        let shape = t.field(f, "shape_id", false);
        t.int(f, "bikes_allowed", false, 0, 2, 0);
        t.int(f, "wheelchair_accessible", false, 0, 2, 0);
        match id {
            None => f.push(
                t.name,
                t.row,
                "MissingKeyError",
                Some("trip_id"),
                None,
                GtfsErrorPriority::Medium,
            ),
            Some(id) => {
                let rec = TripRec {
                    line: t.row,
                    route,
                    shape,
                };
                if f.trips.insert(id.clone(), rec).is_some() {
                    f.push(
                        t.name,
                        t.row,
                        "DuplicateKeyError",
                        Some("trip_id"),
                        Some(&id),
                        GtfsErrorPriority::Medium,
                    );
                }
            }
        }
        t.reference(f, "service_id", true, |f, s| {
            f.services.contains_key(&Some(s.to_string()))
        });
        t.reference(f, "route_id", true, |f, s| f.routes.contains(s));
    })?;

    load_table(f, a, "frequencies", false, |f, t| {
        let trip = t.reference(f, "trip_id", true, |f, s| f.trips.contains_key(s));
        if !trip.is_some_and(|s| f.trips.contains_key(&s)) {
            fails(
                f,
                format!(
                    "frequencies.txt line {}: a trip_id missing from trips.txt makes gtfs-lib \
                 fail (NullPointerException)",
                    t.row
                ),
            );
            return;
        }
        t.time(f, "start_time", true);
        t.time(f, "end_time", true);
        t.int(f, "headway_secs", true, 1, 24 * 60 * 60, 0);
        t.int(f, "exact_times", false, 0, 1, 0);
    })?;

    load_table(f, a, "location_groups", false, |f, t| {
        let id = t.field(f, "location_group_id", true);
        t.field(f, "location_group_name", false);
        f.flex_groups.insert(id);
    })?;
    load_table(f, a, "location_group_stops", false, |f, t| {
        let group = t.field(f, "location_group_id", true);
        let stop = t.field(f, "stop_id", true);
        if !f.flex_groups.contains(&group) {
            fails(
                f,
                format!(
                    "location_group_stops.txt line {}: a location_group_id missing from \
                 location_groups.txt makes gtfs-lib fail (NullPointerException)",
                    t.row
                ),
            );
            return;
        }
        if !stop.is_some_and(|s| f.stops.contains_key(&s)) {
            // FlexGroupStop.Loader.referenceError (no zone can be known
            // here: locations.geojson is refused above)
            f.push(
                t.name,
                t.row,
                "ReferentialIntegrityError",
                Some("stop_id"),
                None,
                GtfsErrorPriority::High,
            );
        }
    })?;

    let mut table_has_flex = false;
    load_table(f, a, "stop_times", true, |f, t| {
        if t.row == 2 {
            table_has_flex = FLEX_FIELDS.iter().any(|c| t.h.col(c).is_some());
        }
        let flex = table_has_flex && FLEX_FIELDS.iter().any(|c| !t.r.get(t.h.col(c)).is_empty());
        if flex {
            t.time(f, "start_pickup_drop_off_window", false);
            t.time(f, "end_pickup_drop_off_window", false);
            let location = t.field(f, "location_id", false);
            let group = t.field(f, "location_group_id", false);
            let pickup_rule = t.field(f, "pickup_booking_rule_id", false);
            let drop_off_rule = t.field(f, "drop_off_booking_rule_id", false);
            // no zones: locations.geojson is refused above
            t.reference(f, "location_id", false, |_, _| false);
            t.reference(f, "location_group_id", false, |f, s| {
                f.flex_groups.contains(&Some(s.to_string()))
            });
            if location.is_some() == group.is_some() {
                f.push(
                    t.name,
                    t.row,
                    "UnsupportedFlexError",
                    None,
                    None,
                    GtfsErrorPriority::Medium,
                );
            }
            let window = |c: &str| {
                let v = t.r.get(t.h.col(c));
                let mut p: Vec<&str> = v.split(':').collect();
                while p.len() > 1 && p.last() == Some(&"") {
                    p.pop();
                }
                !v.is_empty() && p.len() == 3 && p.iter().all(|x| parse_int(x).is_some())
            };
            if !window("start_pickup_drop_off_window") || !window("end_pickup_drop_off_window") {
                f.push(
                    t.name,
                    t.row,
                    "UnsupportedFlexError",
                    None,
                    None,
                    GtfsErrorPriority::Medium,
                );
            }
            if pickup_rule.is_some() || drop_off_rule.is_some() {
                f.push(
                    t.name,
                    t.row,
                    "RestrictedBookingError",
                    Some("*_booking_rule_id"),
                    None,
                    GtfsErrorPriority::Medium,
                );
            }
        }
        let trip = t.field(f, "trip_id", true);
        t.time(f, "arrival_time", false);
        t.time(f, "departure_time", false);
        let stop = t.field(f, "stop_id", false);
        let seq = t.int(f, "stop_sequence", true, 0, i32::MAX, 0);
        t.field(f, "stop_headsign", false);
        let pickup = t.int(f, "pickup_type", false, 0, 3, 0);
        let drop_off = t.int(f, "drop_off_type", false, 0, 3, 0);
        t.double(f, "shape_dist_traveled", false, 0.0, f64::MAX);
        t.int(f, "timepoint", false, 0, 1, INT_MISSING);
        let rec = StopTimeRec {
            line: t.row,
            stop: stop.as_deref().map(|s| f.intern_stop(s)),
            pickup,
            drop_off,
        };
        f.stop_times
            .entry(trip.clone())
            .or_default()
            .insert(seq, rec);
        t.reference(f, "trip_id", true, |f, s| f.trips.contains_key(s));
        if flex {
            f.flex_trips.insert(trip, ());
        } else {
            t.reference(f, "stop_id", true, |f, s| f.stops.contains_key(s));
        }
    })?;

    if let Some(why) = find_patterns_failure(f) {
        fails(f, why);
    }
    post_load(f);
    Ok(())
}

/// `PostLoadValidator.validate`.
fn post_load(f: &mut Feed) {
    // validateCalendarServices
    if f.services.is_empty() {
        f.push(
            "calendar",
            0,
            "GeneralError",
            Some("service_id"),
            None,
            GtfsErrorPriority::Unknown,
        );
    }
    // validateParentStations: (location type, parent requirement, parent's
    // location type); 0 optional, 1 required, 2 forbidden
    const RULES: [(i32, u8, i32); 5] = [(0, 0, 1), (1, 2, 1), (2, 1, 1), (3, 1, 1), (4, 1, 0)];
    let mut found = Vec::new();
    for s in f.stops.values() {
        let Some(&(_, requirement, parent_type)) = RULES.iter().find(|r| r.0 == s.location_type)
        else {
            continue;
        };
        match &s.parent_station {
            None => {
                if requirement == 1 {
                    found.push((s.line, "EmptyFieldError", GtfsErrorPriority::Medium));
                }
            }
            Some(p) => {
                if requirement == 2 {
                    found.push((s.line, "ForbiddenFieldError", GtfsErrorPriority::Medium));
                }
                match f.stops.get(p) {
                    None => {
                        found.push((s.line, "ReferentialIntegrityError", GtfsErrorPriority::High))
                    }
                    Some(parent) => {
                        if parent.location_type != parent_type {
                            found.push((s.line, "RangeError", GtfsErrorPriority::Low));
                        }
                    }
                }
            }
        }
    }
    for (line, kind, p) in found {
        f.push("stops", line, kind, Some("parent_station"), None, p);
    }
    // validateStopPopulationDensity
    let grid = super::population::Grid::earth();
    let mut suspect = Vec::new();
    for (id, s) in &f.stops {
        if !grid.populated(s.lon, s.lat) {
            suspect.push((id.clone(), s.line));
        }
    }
    for (id, line) in suspect {
        f.push(
            "stops",
            line,
            "SuspectStopLocationError",
            Some("stop_id"),
            Some(&id),
            GtfsErrorPriority::Medium,
        );
    }
    // validateFlex
    let flex: Vec<Option<String>> = f.flex_trips.keys().cloned().collect();
    for trip in flex {
        let times: Vec<StopTimeRec> = f
            .stop_times
            .get(&trip)
            .map_or_else(Vec::new, |m| m.values().copied().collect());
        if times.len() != 2 {
            let line = trip
                .as_ref()
                .and_then(|t| f.trips.get(t))
                .map_or(-1, |r| r.line);
            f.push(
                "trips",
                line,
                "UnsupportedFlexError",
                Some("trip_id"),
                None,
                GtfsErrorPriority::Medium,
            );
        }
        for st in times {
            if st.stop.is_some() {
                f.push(
                    "stop_times",
                    st.line,
                    "UnsupportedFlexError",
                    Some("stop_id"),
                    None,
                    GtfsErrorPriority::Medium,
                );
            }
        }
    }
}

/// Java's `String.hashCode`: over the UTF-16 code units, `h * 31 + c`.
fn java_string_hash(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)))
}

/// The first element of a Guava `HashMultimap` value set (a
/// `java.util.HashSet` made for 2 values: a table of 4 slots, doubled when
/// it is more than three quarters full) after adding `ids`, distinct, in
/// this order: the one in the lowest bucket (`h ^ (h >>> 16)` of the
/// string hash), the first added within a bucket.
fn java_hash_set_first<'a>(ids: &[&'a str]) -> Option<&'a str> {
    let mut capacity = 4usize;
    while ids.len() > capacity / 4 * 3 {
        capacity *= 2;
    }
    let mask = (capacity - 1) as u32;
    ids.iter()
        .enumerate()
        .min_by_key(|&(i, id)| {
            let h = java_string_hash(id) as u32;
            ((h ^ (h >> 16)) & mask, i)
        })
        .map(|(_, id)| *id)
}

/// Where `GTFSFeed.findPatterns`, which gtfs-lib runs at the end of
/// loading when no error so far has priority `HIGH`, throws: a pattern's
/// first trip (in its `HashSet`) with a shape of a single point (JTS refuses
/// a line of one point), a stop time without `stop_id` (MapDB refuses a
/// null key), or a pattern whose first or last stop has no `stop_name`
/// (`namePatterns` compares the names).
fn find_patterns_failure(f: &Feed) -> Option<String> {
    if f.errors
        .iter()
        .any(|e| e.priority == GtfsErrorPriority::High)
    {
        return None;
    }
    type Key<'a> = (Option<&'a str>, Vec<Option<u32>>, Vec<i32>, Vec<i32>);
    let mut patterns: HashMap<Key, Vec<&str>> = HashMap::new();
    let mut order: Vec<Key> = Vec::new();
    let mut trip_ids: Vec<&String> = f.trips.keys().collect();
    trip_ids.sort(); // MapDB's map, by trip_id (UTF-16 order for BMP text)
    let none = BTreeMap::new();
    for id in trip_ids {
        let key_trip = Some(id.clone());
        if f.flex_trips.contains_key(&key_trip) {
            continue;
        }
        let times = f.stop_times.get(&key_trip).unwrap_or(&none);
        let key: Key = (
            f.trips[id].route.as_deref(),
            times.values().map(|s| s.stop).collect(),
            times.values().map(|s| s.pickup).collect(),
            times.values().map(|s| s.drop_off).collect(),
        );
        let entry = patterns.entry(key.clone()).or_default();
        if entry.is_empty() {
            order.push(key);
        }
        entry.push(id);
    }
    let stop = |s: u32| f.stops.get(&f.stop_names[s as usize]);
    for key in &order {
        let first = java_hash_set_first(&patterns[key]).expect("a pattern has trips");
        let stops = &key.1;
        // new Pattern(): the first trip's geometry
        if let Some(shape) = &f.trips[first].shape {
            if f.shapes.get(shape).is_some_and(|p| p.len() == 1) {
                return Some(format!(
                    "shape '{shape}' of trip '{first}' has a single point, and gtfs-lib fails \
                     drawing it (IllegalArgumentException)"
                ));
            }
        }
        if stops.len() > 1 && stops.contains(&None) {
            return Some(format!(
                "a stop time of trip '{first}' has no stop_id, and gtfs-lib fails looking it \
                 up (NullPointerException)"
            ));
        }
        // namePatterns()
        let (Some(&from), Some(&to)) = (stops.first(), stops.last()) else {
            continue;
        };
        let (Some(from), Some(to)) = (from, to) else {
            return Some(format!(
                "a stop time of trip '{first}' has no stop_id, and gtfs-lib fails looking it \
                 up (NullPointerException)"
            ));
        };
        if stops.contains(&None) {
            return Some(format!(
                "a stop time of trip '{first}' has no stop_id, and gtfs-lib fails looking it \
                 up (NullPointerException)"
            ));
        }
        for (end, s) in [("first", from), ("last", to)] {
            if stop(s).is_some_and(|r| !r.named) {
                return Some(format!(
                    "stop '{}', the {end} stop of trip '{first}', has no stop_name, and \
                     gtfs-lib fails naming the trip's pattern (NullPointerException)",
                    f.stop_names[s as usize]
                ));
            }
        }
    }
    None
}

/// The errors conveyal gtfs-lib (as in R5 7.6) records while loading the
/// GTFS feed whose `.zip` is `zip`, as r5r 2.5.0 writes them to
/// `gtfs_errors.csv` when it builds a network (one feed; r5r lists the
/// feeds one after the other). `name` names the feed in errors.
///
/// Every check of gtfs-lib's table loaders and of its `PostLoadValidator`
/// is made, in gtfs-lib's order (r5r lists the rows in the order of a Java
/// `HashSet`, which changes from one run to the next; this lists them by
/// table, in gtfs-lib's loading order, then by line). An error when the zip
/// cannot be read, when gtfs-lib would fail on the feed instead of listing
/// its errors (r5r's `build_network()` then fails without writing
/// `gtfs_errors.csv`), and for feeds with GTFS-Flex zones
/// (`locations.geojson`), which peabiru does not read.
pub fn gtfs_errors(name: &str, zip: &[u8]) -> Result<Vec<GtfsFeedError>, crate::NetworkError> {
    match validate(zip) {
        Outcome::Errors(e) => Ok(e),
        Outcome::Fails(why) => Err(crate::NetworkError::gtfs(super::GtfsError::in_feed(
            std::path::Path::new(name),
            super::GtfsError::GtfsLibFails(why),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_as_basic_iso_date() {
        assert_eq!(basic_iso_date("20190513"), Some(20190513));
        assert_eq!(basic_iso_date("20190513Z"), Some(20190513));
        assert_eq!(basic_iso_date("20190513+0300"), Some(20190513));
        assert_eq!(basic_iso_date("20190230"), None);
        assert_eq!(basic_iso_date("2019051"), None);
        assert_eq!(basic_iso_date("2019-05-13"), None);
        assert_eq!(basic_iso_date("20190513+3"), None);
    }

    #[test]
    fn urls_as_java() {
        for ok in [
            "http://www.eptc.com.br",
            "https://a.b/c?d#e",
            "HTTP://x",
            "http://x:8080/",
            "http://[::1]:80/",
            "mailto:a@b",
            "file:/tmp",
            "http://a b",
            "jar:file:/x.jar!/y",
        ] {
            assert!(java_url_ok(ok), "{ok}");
        }
        for bad in [
            "www.eptc.com.br",
            "",
            "foo://x",
            "http://x:ab/",
            "http://[::1/",
            "jar:file:/x.jar",
            "1http://x",
            "/http://x",
        ] {
            assert!(!java_url_ok(bad), "{bad}");
        }
    }

    use crate::gtfs::zip::tests::make_zip;

    type Row = (
        String,
        i32,
        &'static str,
        Option<String>,
        Option<String>,
        &'static str,
    );

    const AGENCY: &str =
        "agency_id,agency_name,agency_url,agency_timezone\n1,A,http://a.com,America/Sao_Paulo\n";
    const STOPS: &str =
        "stop_id,stop_name,stop_lat,stop_lon\nS1,One,-23.55,-46.63\nS2,Two,-23.56,-46.64\n";
    const ST_HEADER: &str = "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n";
    const CALENDAR: &str = "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,\
                            start_date,end_date\nS,1,1,1,1,1,0,0,20190101,20191231\n";

    /// A small clean feed with `changes`: a table replaced or added (an
    /// empty text drops it).
    fn check(changes: &[(&str, String)]) -> Result<Vec<Row>, String> {
        let base = [
            ("agency.txt", AGENCY.to_string()),
            ("stops.txt", STOPS.to_string()),
            (
                "routes.txt",
                "route_id,agency_id,route_short_name,route_type\nR,1,R,3\n".into(),
            ),
            ("trips.txt", "route_id,service_id,trip_id\nR,S,T\n".into()),
            (
                "stop_times.txt",
                format!("{ST_HEADER}T,08:00:00,08:00:00,S1,1\nT,08:10:00,08:10:00,S2,2\n"),
            ),
            ("calendar.txt", CALENDAR.to_string()),
        ];
        let mut files: Vec<(&str, &[u8])> = Vec::new();
        for (name, text) in base.iter().chain(changes) {
            files.retain(|f| f.0 != *name);
            if !text.is_empty() {
                files.push((name, text.as_bytes()));
            }
        }
        match validate(&make_zip(&files, true)) {
            Outcome::Fails(why) => Err(why),
            Outcome::Errors(e) => Ok(e
                .into_iter()
                .map(|e| {
                    (
                        e.file().unwrap().to_string(),
                        e.line(),
                        e.error_type(),
                        e.field().map(str::to_string),
                        e.id().map(str::to_string),
                        e.priority().as_str(),
                    )
                })
                .collect()),
        }
    }

    fn row(
        file: &str,
        line: i32,
        kind: &'static str,
        field: Option<&str>,
        id: Option<&str>,
    ) -> Row {
        let p = match kind {
            "RangeError" | "URLParseError" => "LOW",
            _ => "MEDIUM",
        };
        (
            file.into(),
            line,
            kind,
            field.map(str::to_string),
            id.map(str::to_string),
            p,
        )
    }

    #[test]
    fn a_clean_feed_has_no_errors() {
        assert_eq!(check(&[]), Ok(vec![]));
    }

    /// gtfs-lib's rows for these cases, as r5r 2.5.0 writes them to
    /// gtfs_errors.csv (checked against it with larger feeds of the same
    /// kinds).
    #[test]
    fn loader_errors_as_gtfs_lib() {
        let got = check(&[
            ("agency.txt", format!("{AGENCY}1,A,www.a.com,\n")),
            (
                "stops.txt",
                format!("{STOPS}S3,,-95,-46.64\nS4,Sea,-30.0,-30.0\n"),
            ),
            (
                "calendar_dates.txt",
                "service_id,date,exception_type\nS,20190513,3\nS,20190513,1\n".into(),
            ),
            (
                "frequencies.txt",
                "trip_id,start_time,end_time,headway_secs\nT,8:00,09:00:00:,0\n".into(),
            ),
            ("empty.txt", String::new()),
            (
                "transfers.txt",
                "from_stop_id,to_stop_id,transfer_type\n,S1,1\n".into(),
            ),
        ])
        .unwrap();
        let want = vec![
            row("agency", 3, "URLParseError", Some("agency_url"), None),
            row(
                "agency",
                3,
                "EmptyFieldError",
                Some("agency_timezone"),
                None,
            ),
            row(
                "agency",
                3,
                "DuplicateKeyError",
                Some("agency_id"),
                Some("1"),
            ),
            row("calendar_dates", 2, "RangeError", None, None),
            row(
                "calendar_dates",
                3,
                "DuplicateKeyError",
                Some("(service_id, date)"),
                Some("(S,2019-05-13)"),
            ),
            row("stops", 4, "EmptyFieldError", Some("stop_name"), None),
            row("stops", 4, "RangeError", None, None),
            // read twice: as a field and for the reference check
            row(
                "transfers",
                2,
                "EmptyFieldError",
                Some("from_stop_id"),
                None,
            ),
            row(
                "transfers",
                2,
                "EmptyFieldError",
                Some("from_stop_id"),
                None,
            ),
            row("frequencies", 2, "TimeParseError", Some("start_time"), None),
            // "09:00:00:" has three parts for Java's split(); 0 is out of range
            row("frequencies", 2, "RangeError", None, None),
            row(
                "stops",
                4,
                "SuspectStopLocationError",
                Some("stop_id"),
                Some("S3"),
            ),
            row(
                "stops",
                5,
                "SuspectStopLocationError",
                Some("stop_id"),
                Some("S4"),
            ),
        ];
        assert_eq!(got, want);
    }

    #[test]
    fn high_priority_errors_as_gtfs_lib() {
        let got = check(&[
            (
                "stops.txt",
                "stop_id,stop_name,stop_lat,stop_lon\nS1,One,-23.55,-46.63\nS2,Two,x,-46.64\n"
                    .into(),
            ),
            (
                "trips.txt",
                "route_id,service_id,trip_id\nR,S,T\nQ,S,T2\nR,NONE,T3\n".into(),
            ),
            ("fare_rules.txt", "fare_id\nF\n".into()),
        ])
        .unwrap();
        let high: Vec<_> = got
            .iter()
            .filter(|r| r.5 == "HIGH")
            .map(|r| (r.0.as_str(), r.1, r.2, r.3.as_deref()))
            .collect();
        assert_eq!(
            high,
            vec![
                (
                    "fare_rules",
                    2,
                    "ReferentialIntegrityError",
                    Some("fare_id")
                ),
                ("stops", 3, "NumberParseError", Some("stop_lat")),
                ("trips", 3, "ReferentialIntegrityError", Some("route_id")),
                ("trips", 4, "ReferentialIntegrityError", Some("service_id")),
            ]
        );
        // a table in a subfolder: found there, and also missing
        let sub = check(&[
            ("agency.txt", String::new()),
            ("gtfs/agency.txt", AGENCY.into()),
        ])
        .unwrap();
        let mut high = row("agency", 0, "TableInSubdirectoryError", None, None);
        high.5 = "HIGH";
        assert_eq!(
            sub,
            vec![high, row("agency", 0, "MissingTableError", None, None)]
        );
    }

    #[test]
    fn feeds_gtfs_lib_fails_on() {
        let date = check(&[(
            "calendar_dates.txt",
            "service_id,date,exception_type\nS,20190230,1\n".into(),
        )]);
        assert!(date.unwrap_err().contains("DateTimeParseException"));
        let name = check(&[(
            "stops.txt",
            "stop_id,stop_name,stop_lat,stop_lon\nS1,,-23.55,-46.63\nS2,Two,-23.56,-46.64\n".into(),
        )]);
        assert!(name.unwrap_err().contains("stop_name"));
        // a stop without a name in the middle of a trip is fine
        let mid = check(&[
            ("stops.txt", format!("{STOPS}S3,,-23.57,-46.65\n")),
            (
                "stop_times.txt",
                format!(
                    "{ST_HEADER}T,08:00:00,08:00:00,S1,1\nT,08:05:00,08:05:00,S3,2\n\
                     T,08:10:00,08:10:00,S2,3\n"
                ),
            ),
        ]);
        assert_eq!(mid.unwrap().len(), 1); // the empty stop_name
        let one_point = check(&[
            (
                "trips.txt",
                "route_id,service_id,trip_id,shape_id\nR,S,T,P\n".into(),
            ),
            (
                "shapes.txt",
                "shape_id,shape_pt_lat,shape_pt_lon,shape_pt_sequence\nP,-23.55,-46.63,1\n".into(),
            ),
        ]);
        assert!(one_point.unwrap_err().contains("single point"));
        let freq = check(&[(
            "frequencies.txt",
            "trip_id,start_time,end_time,headway_secs\nNOPE,08:00:00,09:00:00,600\n".into(),
        )]);
        assert!(freq.unwrap_err().contains("frequencies.txt line 2"));
    }

    #[test]
    fn hash_set_first_as_java() {
        // "Aa" and "BB" share Java's hash 2112: insertion order decides
        assert_eq!(java_string_hash("Aa"), java_string_hash("BB"));
        assert_eq!(java_hash_set_first(&["BB", "Aa"]), Some("BB"));
        assert_eq!(java_string_hash("T1"), 2653);
        // buckets of a 4-slot table: "T3" 2655 & 3 = 3, "T4" 2656 & 3 = 0
        assert_eq!(java_hash_set_first(&["T3", "T4"]), Some("T4"));
        assert_eq!(java_hash_set_first(&["T1", "T2"]), Some("T1"));
    }
}
