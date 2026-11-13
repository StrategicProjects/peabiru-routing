//! Results written to CSV files, one per origin (`output_dir`).

use crate::{Accessibility, ExpandedMatrix, StreetMatrix, TransitMatrix};
use std::fmt::Write as _;
use std::io;
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// `System.lineSeparator()`, which Java's `PrintWriter.println` writes.
pub(crate) const LINE_END: &str = if cfg!(windows) { "\r\n" } else { "\n" };

/// Why `id` cannot be part of a file name `<prefix><id>...csv` in the
/// output directory, `None` when it can. Refused: path separators (`/`,
/// `\`) and NUL, which would name a file elsewhere or none; on Windows also
/// the characters it does not allow in file names (`< > : " | ? *` and the
/// control characters). The prefix (`from_`) keeps the name from being
/// `.`, `..` or a reserved device name, and an empty id gives `from_.csv`.
pub(crate) fn id_file_name_problem(id: &str) -> Option<&'static str> {
    if id.contains(['/', '\\']) {
        return Some("a path separator");
    }
    if id.contains('\0') {
        return Some("a NUL character");
    }
    if cfg!(windows)
        && id
            .chars()
            .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || (c as u32) < 32)
    {
        return Some("a character Windows does not allow in file names");
    }
    None
}

/// [`RouteError::Invalid`](crate::RouteError::Invalid) naming the ids that
/// cannot be part of the file names the output directory's files are
/// named after (see [`id_file_name_problem`]).
pub(crate) fn check_file_ids<'a>(
    ids: impl IntoIterator<Item = &'a str>,
) -> Result<(), crate::RouteError> {
    let mut bad: Vec<String> = Vec::new();
    let mut why = "";
    for id in ids {
        if let Some(w) = id_file_name_problem(id) {
            let shown = format!("{id:?}");
            if !bad.contains(&shown) {
                if bad.is_empty() {
                    why = w;
                }
                bad.push(shown);
            }
        }
    }
    if bad.is_empty() {
        return Ok(());
    }
    Err(crate::RouteError::Invalid(format!(
        "with output_dir the file names are built from the ids (from_<id>.csv), and {} cannot \
         be part of a file name in the directory ({}{}): {}",
        if bad.len() == 1 {
            "this id"
        } else {
            "these ids"
        },
        why,
        if bad.len() > 1 {
            ", or another character not allowed"
        } else {
            ""
        },
        bad.join(", ")
    )))
}

/// Values (matrix cells, or rows of an expanded matrix) routed at a time
/// by [`CsvOutput::origin_chunks`].
const CHUNK_VALUES: usize = 1 << 23;

/// Fewest origins per slice of [`CsvOutput::origin_chunks`], so that the
/// workers stay busy when there are many destinations.
const MIN_CHUNK_ORIGINS: usize = 64;

/// Writes routing results to a directory, one CSV file per origin.
///
/// Each origin's table is saved to a file of its own as soon as the origin
/// is routed, and nothing is kept in memory (the ported
/// `R5DataFrameProcess.tryRunProcess` and `RDataFrame.saveToCsv`):
///
/// - the file is `<output_dir>/from_<origin id>.csv` (plain concatenation,
///   the id is not sanitized), written only when the origin has at least
///   one row, replacing a file of the same name;
/// - a header with the column names joined by `,`, then one line per row,
///   each ended by the platform's line separator (`\n`, `\r\n` on Windows);
/// - a field holding `,`, `"`, `\n` or `\r` is quoted, with `"` doubled; a
///   missing value is an empty field;
/// - integers are written in decimal, doubles as Java's
///   `Double.toString` writes them (`0.0`, `2.1`, `1.0E-4`).
///
/// The values are the tables as routed, with nothing done to them
/// afterwards: the WALK origin/destination swap is not made, and the
/// accessibility `cutoff` column is kept with `fixed_exponential` (the
/// placeholder `0`).
///
/// The routing functions return whole matrices; to keep memory bounded,
/// route the origins in the slices that [`CsvOutput::origin_chunks`] gives
/// and write each matrix before routing the next slice.
///
/// The ids are those of all the origins and destinations of the call;
/// each `write_*` method takes a matrix routed from a slice of the origins
/// that starts at `first_origin`.
#[derive(Debug, Clone)]
pub struct CsvOutput<'a> {
    dir: String,
    origin_ids: &'a [String],
    destination_ids: &'a [String],
    n_threads: usize,
}

impl<'a> CsvOutput<'a> {
    /// A writer to directory `dir`, which must exist, for the origins and
    /// destinations with these ids (the destination ids are not used by
    /// [`write_accessibility`](CsvOutput::write_accessibility)). Files are
    /// written by `n_threads` threads (0 = all cores).
    ///
    /// The files are named after the origin ids, so an id that cannot be
    /// part of a file name in `dir` (one holding a path separator `/` or
    /// `\`, or NUL; on Windows also `< > : " | ? *` or a control
    /// character) is [`RouteError::Invalid`](crate::RouteError::Invalid),
    /// before anything is routed or written.
    pub fn new(
        dir: &str,
        origin_ids: &'a [String],
        destination_ids: &'a [String],
        n_threads: usize,
    ) -> Result<Self, crate::RouteError> {
        check_file_ids(origin_ids.iter().map(String::as_str))?;
        Ok(CsvOutput {
            dir: dir.to_owned(),
            origin_ids,
            destination_ids,
            n_threads,
        })
    }

    /// The path of the file of origin `origin`: `<dir>/from_<id>.csv`.
    pub fn path(&self, origin: usize) -> String {
        format!("{}/from_{}.csv", self.dir, self.origin_ids[origin])
    }

    /// The origins in consecutive slices, each small enough that its matrix
    /// holds about 8 million values (but at least 64 origins) when an
    /// origin has `values_per_origin` of them: destinations, times
    /// percentiles for a travel-time matrix; destinations times departure
    /// minutes (times draws) for an expanded matrix.
    pub fn origin_chunks(&self, values_per_origin: usize) -> impl Iterator<Item = Range<usize>> {
        let n = self.origin_ids.len();
        let size = (CHUNK_VALUES / values_per_origin.max(1)).max(MIN_CHUNK_ORIGINS);
        (0..n).step_by(size).map(move |s| s..(s + size).min(n))
    }

    /// A street travel-time matrix: `from_id`, `to_id` and one
    /// `travel_time_pXX` column per percentile (the street time repeated
    /// in each), one row per destination reached. `percentiles` are the
    /// column names' numbers, in order.
    pub fn write_street_matrix(
        &self,
        first_origin: usize,
        m: &StreetMatrix,
        percentiles: &[i32],
    ) -> io::Result<()> {
        let header = ttm_header(percentiles);
        self.write_origins(first_origin, m.n_origins(), &header, |i, out| {
            let from = &self.origin_ids[first_origin + i];
            for (j, v) in m.row(i).iter().enumerate() {
                let Some(v) = v else { continue };
                start_row(out, from, &self.destination_ids[j]);
                for _ in percentiles {
                    let _ = write!(out, ",{v}");
                }
                out.push_str(LINE_END);
            }
        })
    }

    /// A walk + transit travel-time matrix: `from_id`, `to_id` and one
    /// `travel_time_pXX` column per percentile, one row per destination
    /// whose lowest percentile is reported, empty where there is no time.
    pub fn write_transit_matrix(&self, first_origin: usize, m: &TransitMatrix) -> io::Result<()> {
        let np = m.percentiles().len();
        let header = ttm_header(m.percentiles());
        self.write_origins(first_origin, m.n_origins(), &header, |i, out| {
            let from = &self.origin_ids[first_origin + i];
            for (j, cell) in m.row(i).chunks_exact(np.max(1)).enumerate() {
                if np == 0 || cell[0].is_none() {
                    continue;
                }
                start_row(out, from, &self.destination_ids[j]);
                for v in cell {
                    out.push(',');
                    if let Some(v) = v {
                        let _ = write!(out, "{v}");
                    }
                }
                out.push_str(LINE_END);
            }
        })
    }

    /// An expanded or arrival matrix: `from_id`, `to_id`, `departure_time`
    /// (`HH:MM:SS`), `draw_number`, with `breakdown` the five partial times,
    /// then `routes`, with `breakdown` `n_rides`, and `total_time`; the
    /// trip's fields are empty where there is no trip.
    pub fn write_expanded(
        &self,
        first_origin: usize,
        m: &ExpandedMatrix,
        breakdown: bool,
    ) -> io::Result<()> {
        let header = if breakdown {
            "from_id,to_id,departure_time,draw_number,access_time,wait_time,ride_time,\
             transfer_time,egress_time,routes,n_rides,total_time"
        } else {
            "from_id,to_id,departure_time,draw_number,routes,total_time"
        };
        self.write_origins(first_origin, m.n_origins(), header, |i, out| {
            let from = &self.origin_ids[first_origin + i];
            for r in m.origin(i) {
                start_row(out, from, &self.destination_ids[r.to as usize]);
                // Utils.getTimeFromSeconds: hours are not wrapped at 24
                let s = r.departure;
                let _ = write!(
                    out,
                    ",{:02}:{:02}:{:02},{}",
                    s / 3600,
                    s / 60 % 60,
                    s % 60,
                    r.draw
                );
                match &r.trip {
                    Some(t) => {
                        if breakdown {
                            for v in [
                                t.access_time,
                                t.wait_time,
                                t.ride_time,
                                t.transfer_time,
                                t.egress_time,
                            ] {
                                out.push(',');
                                java_double(v, out);
                            }
                        }
                        out.push(',');
                        csv_field(&t.routes, out);
                        if breakdown {
                            let _ = write!(out, ",{}", t.n_rides);
                        }
                        out.push(',');
                        java_double(t.total_time, out);
                    }
                    None => out.push_str(if breakdown { ",,,,,,,," } else { ",," }),
                }
                out.push_str(LINE_END);
            }
        })
    }

    /// Accessibility: `id`, `opportunity`, `percentile`, `cutoff` and
    /// `accessibility`, one row per opportunity column (named by
    /// `opportunities`, in the columns' order), percentile and cutoff.
    pub fn write_accessibility(
        &self,
        first_origin: usize,
        a: &Accessibility,
        opportunities: &[String],
    ) -> io::Result<()> {
        let header = "id,opportunity,percentile,cutoff,accessibility";
        self.write_origins(first_origin, a.n_origins(), header, |i, out| {
            let id = &self.origin_ids[first_origin + i];
            for (k, name) in opportunities.iter().enumerate().take(a.n_opportunities()) {
                for (p, pct) in a.percentiles().iter().enumerate() {
                    for (c, cutoff) in a.cutoffs().iter().enumerate() {
                        csv_field(id, out);
                        out.push(',');
                        csv_field(name, out);
                        let _ = write!(out, ",{pct},{cutoff},");
                        java_double(a.get(i, k, p, c), out);
                        out.push_str(LINE_END);
                    }
                }
            }
        })
    }

    /// Write the file of each of the `n` origins from `first_origin` whose
    /// `rows(i, buffer)` (`i` from 0) adds at least one line, on the
    /// writer's threads. The first error met is returned, after the
    /// threads have stopped.
    fn write_origins(
        &self,
        first_origin: usize,
        n: usize,
        header: &str,
        rows: impl Fn(usize, &mut String) + Sync,
    ) -> io::Result<()> {
        assert!(
            first_origin + n <= self.origin_ids.len(),
            "origins {first_origin}..{} of {}",
            first_origin + n,
            self.origin_ids.len()
        );
        let next = AtomicUsize::new(0);
        let failed: Mutex<Option<io::Error>> = Mutex::new(None);
        let threads = crate::street::router::worker_threads(self.n_threads, n);
        crate::sys::scoped(0..threads, |_| {
            let mut out = String::new();
            loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= n || failed.lock().map_or(true, |f| f.is_some()) {
                    break;
                }
                out.clear();
                out.push_str(header);
                out.push_str(LINE_END);
                let empty = out.len();
                rows(i, &mut out);
                if out.len() == empty {
                    continue;
                }
                let path = self.path(first_origin + i);
                if let Err(e) = std::fs::write(&path, out.as_bytes()) {
                    let e = io::Error::new(e.kind(), format!("cannot write '{path}': {e}"));
                    if let Ok(mut f) = failed.lock() {
                        f.get_or_insert(e);
                    }
                    break;
                }
            }
        });
        match failed.into_inner() {
            Ok(Some(e)) => Err(e),
            _ => Ok(()),
        }
    }
}

fn ttm_header(percentiles: &[i32]) -> String {
    let mut h = String::from("from_id,to_id");
    for p in percentiles {
        let _ = write!(h, ",travel_time_p{p:02}");
    }
    h
}

fn start_row(out: &mut String, from: &str, to: &str) {
    csv_field(from, out);
    out.push(',');
    csv_field(to, out);
}

/// `RDataFrame.csvField`: quoted, with `"` doubled, when it holds `,`,
/// `"`, `\n` or `\r`.
pub(crate) fn csv_field(s: &str, out: &mut String) {
    if s.contains([',', '"', '\n', '\r']) {
        out.push('"');
        out.push_str(&s.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(s);
    }
}

/// Java's `Double.toString(double)` (JDK 19 and later, Raffaello
/// Giulietti's algorithm): the shortest decimal that rounds to `v` (at
/// least two significant digits are considered when one would do, the
/// closest to `v`), in plain notation from 10^-3 up to 10^7 and as
/// `d.dddE±n` otherwise, always with a digit after the point.
pub(crate) fn java_double(v: f64, out: &mut String) {
    if v.is_nan() {
        out.push_str("NaN");
        return;
    }
    if v.is_sign_negative() {
        out.push('-');
    }
    let a = v.abs();
    if a.is_infinite() {
        out.push_str("Infinity");
        return;
    }
    if a == 0.0 {
        out.push_str("0.0");
        return;
    }
    // The length of the shortest decimal that rounds to `a` (Rust's `{:e}`),
    // but at least 2. Of the decimals of that length Java takes the one
    // closest to `a`, the even one on a tie: `a` correctly rounded to that
    // many digits (Rust rounds exact ties to even). Rust's shortest output
    // may be another one of the same length (on a tie), and with one digit
    // Java looks at two (5e-324 is 4.9E-324).
    let (mut digits, mut exp) = sci_digits(&format!("{a:e}"));
    let len = digits.len().max(2);
    let closest = format!("{a:.prec$e}", prec = len - 1);
    if closest.parse::<f64>() == Ok(a) {
        let (d, e) = sci_digits(&closest);
        digits = d.trim_end_matches('0').to_owned();
        exp = e;
    }
    let n = digits.len() as i32;
    if (-3..7).contains(&exp) {
        if exp >= 0 {
            let int_len = (exp + 1) as usize;
            if digits.len() <= int_len {
                out.push_str(&digits);
                out.extend(std::iter::repeat('0').take(int_len - digits.len()));
                out.push_str(".0");
            } else {
                out.push_str(&digits[..int_len]);
                out.push('.');
                out.push_str(&digits[int_len..]);
            }
        } else {
            out.push_str("0.");
            out.extend(std::iter::repeat('0').take((-exp - 1) as usize));
            out.push_str(&digits);
        }
    } else {
        out.push_str(&digits[..1]);
        out.push('.');
        if n > 1 {
            out.push_str(&digits[1..]);
        } else {
            out.push('0');
        }
        let _ = write!(out, "E{exp}");
    }
}

/// The significant digits (no point) and the exponent of Rust's `{:e}`
/// format of a positive finite double, such as `2.1e0`.
fn sci_digits(s: &str) -> (String, i32) {
    let (mantissa, exp) = s.split_once('e').expect("{:e} has an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    (digits, exp.parse().expect("{:e} exponent"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_that_cannot_name_a_file_in_the_directory_are_refused() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // ids that name a file, as they are
        for ok in ["", "a b", "a,b", "ação", "..", ".", "CON", "x."] {
            assert!(id_file_name_problem(ok).is_none(), "{ok:?}");
        }
        let posix_ok = ["a\"b", "a:b", "a*b", "a?b", "a<b>", "a|b"];
        for id in posix_ok {
            assert_eq!(id_file_name_problem(id).is_some(), cfg!(windows), "{id:?}");
        }
        for (id, why) in [
            ("a/b", "a path separator"),
            ("../x", "a path separator"),
            ("a\\b", "a path separator"),
            ("/abs", "a path separator"),
            ("a\0b", "a NUL character"),
        ] {
            assert_eq!(id_file_name_problem(id), Some(why), "{id:?}");
        }
        let (o, d) = (ids(&["p", "../x", "q", "../x"]), ids(&["p"]));
        let e = CsvOutput::new("out", &o, &d, 1).unwrap_err().to_string();
        assert!(e.contains("\"../x\"") && e.contains("from_<id>.csv"), "{e}");
        assert_eq!(e.matches("../x").count(), 1, "{e}");
        assert!(
            CsvOutput::new("out", &d, &o, 1).is_ok(),
            "destination ids name no file"
        );
        // the writers of pairs check both ids, before writing anything
        let pairs = vec![
            ("a".to_string(), "b".to_string()),
            ("c".into(), "x\\y".into()),
        ];
        let e = crate::write_itineraries_csv("/nonexistent", &pairs, &[], &[], true, false)
            .unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
        assert!(e.to_string().contains("\"x\\\\y\""), "{e}");
        let e = crate::write_trip_options_csv("/nonexistent", &pairs, &[], &[], true, false)
            .unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
    }

    fn jd(v: f64) -> String {
        let mut s = String::new();
        java_double(v, &mut s);
        s
    }

    #[test]
    fn doubles_as_java_writes_them() {
        // values checked against Double.toString of OpenJDK 21
        let cases = [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (2.1, "2.1"),
            (71.8, "71.8"),
            (577.0, "577.0"),
            (100.0, "100.0"),
            (2189.822126741116, "2189.822126741116"),
            (0.10912451129259718, "0.10912451129259718"),
            (0.001, "0.001"),
            (0.0001, "1.0E-4"),
            (0.00012345, "1.2345E-4"),
            (9999999.0, "9999999.0"),
            (1.0e7, "1.0E7"),
            (12345678.9, "1.23456789E7"),
            (1.0e21, "1.0E21"),
            (1.0e-300, "1.0E-300"),
            (f64::MIN_POSITIVE, "2.2250738585072014E-308"),
            (5e-324, "4.9E-324"),
            (f64::MAX, "1.7976931348623157E308"),
            (0.1 + 0.2, "0.30000000000000004"),
            (-3.5, "-3.5"),
            (2e23, "2.0E23"),
            (1e23, "1.0E23"),
            (9.9999e6, "9999900.0"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            (f64::NAN, "NaN"),
        ];
        for (v, want) in cases {
            assert_eq!(jd(v), want, "{v:e}");
        }
    }

    #[test]
    fn ties_go_to_the_even_digit_as_in_java() {
        // two shortest decimals equally close: Java takes the even one
        for (bits, want) in [
            (0x42d2_c3ca_6e92_2ae8_u64, "8.252849666679562E13"),
            (0xc315_fccb_02cd_ffe1, "-1.5472308416348082E15"),
            (0x428e_48f5_7aea_cb40, "4.1623381190014062E12"),
        ] {
            assert_eq!(jd(f64::from_bits(bits)), want);
        }
    }

    #[test]
    fn fields_are_quoted_as_fwrite_quotes_them() {
        let q = |s: &str| {
            let mut o = String::new();
            csv_field(s, &mut o);
            o
        };
        assert_eq!(q("89a901291abffff"), "89a901291abffff");
        assert_eq!(q(""), "");
        assert_eq!(q("a,b"), "\"a,b\"");
        assert_eq!(q("q\"x"), "\"q\"\"x\"");
        assert_eq!(q("a\nb"), "\"a\nb\"");
        assert_eq!(q("a b"), "a b");
    }

    #[test]
    fn origins_are_routed_in_bounded_slices() {
        let ids: Vec<String> = (0..1000).map(|i| i.to_string()).collect();
        let out = CsvOutput::new("x", &ids, &ids, 1).unwrap();
        let all: Vec<_> = out.origin_chunks(1000).collect();
        assert_eq!(all, vec![0..1000]);
        let big: Vec<_> = out.origin_chunks(1 << 20).collect();
        assert_eq!(big.len(), 16);
        assert_eq!(big[0], 0..64);
        assert_eq!(big[15], 960..1000);
        assert_eq!(
            CsvOutput::new("x", &[], &[], 1)
                .unwrap()
                .origin_chunks(5)
                .count(),
            0
        );
    }
}
