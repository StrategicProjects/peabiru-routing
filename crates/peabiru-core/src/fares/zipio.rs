//! r5r's fare structure file: a `.zip` holding five CSV files, written by
//! `data.table::fwrite()` and read back by `data.table::fread()`
//! (`write_fare_structure()`, `read_fare_structure()`).
//!
//! The CSV text is `fwrite`'s (its quoting, its numbers, `\n` line ends);
//! the archive around it is deflated as `zip::zip()`'s, with the same
//! entries in the same order, but dated 1980-01-01 and compressed by
//! miniz_oxide, so the `.zip` bytes differ from r5r's (whose bytes carry
//! the files' modification times anyway) while every file in it is the
//! same.

use super::{FarePerRoute, FarePerTransfer, FarePerType, FareStructure};
use crate::gtfs::zip::Archive;
use crate::RouteError;

/// A column of a table written as `fwrite` writes it.
#[derive(Debug, Clone)]
pub enum FwriteColumn {
    /// Character; `None` is `NA` (written empty).
    Text(Vec<Option<String>>),
    /// Double; NaN is `NA` (written empty).
    Number(Vec<f64>),
    /// Integer; `None` is `NA`.
    Integer(Vec<Option<i32>>),
    /// Logical; `None` is `NA`.
    Logical(Vec<Option<bool>>),
}

impl FwriteColumn {
    fn len(&self) -> usize {
        match self {
            FwriteColumn::Text(v) => v.len(),
            FwriteColumn::Number(v) => v.len(),
            FwriteColumn::Integer(v) => v.len(),
            FwriteColumn::Logical(v) => v.len(),
        }
    }
}

/// `fwrite`'s text of a character field (`quote = "auto"`): quoted, with
/// quotes doubled, when it holds a comma, a quote or a line end, or is
/// empty (an empty string is `""`, `NA` nothing).
fn fwrite_text(s: &str, out: &mut String) {
    if s.is_empty() || s.contains([',', '"', '\n', '\r']) {
        out.push('"');
        out.push_str(&s.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(s);
    }
}

/// `fwrite`'s text of a double: up to 15 significant digits, trailing
/// zeros dropped, in fixed or scientific notation, whichever is shorter
/// (fixed on a tie; `scipen = 0`).
pub(crate) fn fwrite_number(x: f64, out: &mut String) {
    if x.is_nan() {
        return;
    }
    if x.is_infinite() {
        out.push_str(if x > 0.0 { "Inf" } else { "-Inf" });
        return;
    }
    if x == 0.0 {
        out.push('0');
        return;
    }
    // 15 significant digits, correctly rounded
    let e = format!("{:.14e}", x);
    let (mant, exp) = e.split_once('e').expect("exponent");
    let exp: i32 = exp.parse().expect("exponent");
    let neg = mant.starts_with('-');
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let nd = digits.len() as i32;
    // fixed notation
    let mut fixed = String::new();
    if exp >= 0 {
        if nd <= exp + 1 {
            fixed.push_str(digits);
            for _ in nd..exp + 1 {
                fixed.push('0');
            }
        } else {
            fixed.push_str(&digits[..(exp + 1) as usize]);
            fixed.push('.');
            fixed.push_str(&digits[(exp + 1) as usize..]);
        }
    } else {
        fixed.push_str("0.");
        for _ in 0..(-exp - 1) {
            fixed.push('0');
        }
        fixed.push_str(digits);
    }
    // scientific notation, at least two exponent digits
    let mut sci = String::new();
    sci.push_str(&digits[..1]);
    if nd > 1 {
        sci.push('.');
        sci.push_str(&digits[1..]);
    }
    sci.push('e');
    sci.push(if exp < 0 { '-' } else { '+' });
    let a = exp.unsigned_abs();
    if a < 10 {
        sci.push('0');
    }
    sci.push_str(&a.to_string());
    if neg {
        out.push('-');
    }
    out.push_str(if fixed.len() > sci.len() {
        &sci
    } else {
        &fixed
    });
}

/// A table as `fwrite(x, file)` writes it: a header of the column names,
/// then one line per row, `\n` line ends.
pub fn fwrite_csv(columns: &[(String, FwriteColumn)]) -> Result<String, RouteError> {
    let n = columns.first().map_or(0, |c| c.1.len());
    if columns.iter().any(|c| c.1.len() != n) {
        return Err(RouteError::Invalid(
            "the columns of a fare structure table differ in length".into(),
        ));
    }
    let mut s = String::new();
    for (k, (name, _)) in columns.iter().enumerate() {
        if k > 0 {
            s.push(',');
        }
        fwrite_text(name, &mut s);
    }
    s.push('\n');
    for i in 0..n {
        for (k, (_, col)) in columns.iter().enumerate() {
            if k > 0 {
                s.push(',');
            }
            match col {
                FwriteColumn::Text(v) => {
                    if let Some(t) = &v[i] {
                        fwrite_text(t, &mut s);
                    }
                }
                FwriteColumn::Number(v) => fwrite_number(v[i], &mut s),
                FwriteColumn::Integer(v) => {
                    if let Some(x) = v[i] {
                        s.push_str(&x.to_string());
                    }
                }
                FwriteColumn::Logical(v) => {
                    if let Some(b) = v[i] {
                        s.push_str(if b { "TRUE" } else { "FALSE" });
                    }
                }
            }
        }
        s.push('\n');
    }
    Ok(s)
}

/// CRC-32 (IEEE) of `data`.
fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, t) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *t = c;
    }
    let mut c = !0u32;
    for &b in data {
        c = table[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

/// A `.zip` archive of `files` (name, bytes), each entry deflated (level 9,
/// as `zip::zip()`) and dated 1980-01-01.
pub fn zip_deflated(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let le16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
    let le32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    for (name, data) in files {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let packed = miniz_oxide::deflate::compress_to_vec(data, 9);
        let (n, len, size) = (name.len() as u16, data.len() as u32, packed.len() as u32);
        le32(&mut out, 0x0403_4b50);
        le16(&mut out, 20); // version needed
        le16(&mut out, 0x0800); // UTF-8 names
        le16(&mut out, 8); // deflated
        le16(&mut out, 0); // time
        le16(&mut out, 0x21); // date 1980-01-01
        le32(&mut out, crc);
        le32(&mut out, size);
        le32(&mut out, len);
        le16(&mut out, n);
        le16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&packed);
        le32(&mut central, 0x0201_4b50);
        le16(&mut central, 0x031e); // made by: Unix
        le16(&mut central, 20);
        le16(&mut central, 0x0800);
        le16(&mut central, 8);
        le16(&mut central, 0);
        le16(&mut central, 0x21);
        le32(&mut central, crc);
        le32(&mut central, size);
        le32(&mut central, len);
        le16(&mut central, n);
        le16(&mut central, 0); // extra
        le16(&mut central, 0); // comment
        le16(&mut central, 0); // disk
        le16(&mut central, 0); // internal attributes
        le32(&mut central, 0o100644 << 16); // -rw-r--r--
        le32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    let cd_len = central.len() as u32;
    out.extend_from_slice(&central);
    le32(&mut out, 0x0605_4b50);
    le16(&mut out, 0);
    le16(&mut out, 0);
    le16(&mut out, files.len() as u16);
    le16(&mut out, files.len() as u16);
    le32(&mut out, cd_len);
    le32(&mut out, cd_offset);
    le16(&mut out, 0);
    out
}

/// The five file names of r5r's archive, in its order.
pub(crate) const FILES: [&str; 5] = [
    "global_settings.csv",
    "fares_per_type.csv",
    "fares_per_transfer.csv",
    "fares_per_route.csv",
    "debug_settings.csv",
];

fn text(v: impl IntoIterator<Item = Option<String>>) -> FwriteColumn {
    FwriteColumn::Text(v.into_iter().collect())
}

/// The five CSV files of `write_fare_structure()`, in its order, for a
/// structure whose tables have r5r's columns.
pub(crate) fn fare_csvs(s: &FareStructure) -> Result<Vec<String>, RouteError> {
    let col = |n: &str, c: FwriteColumn| (n.to_string(), c);
    let global = fwrite_csv(&[
        col(
            "setting",
            text(
                [
                    "max_discounted_transfers",
                    "transfer_time_allowance",
                    "fare_cap",
                ]
                .map(|s| Some(s.to_string())),
            ),
        ),
        col(
            "value",
            FwriteColumn::Number(vec![
                s.max_discounted_transfers,
                s.transfer_time_allowance,
                s.fare_cap,
            ]),
        ),
    ])?;
    let t = &s.fares_per_type;
    let types = fwrite_csv(&[
        col("type", text(t.iter().map(|x| x.fare_type.clone()))),
        col(
            "unlimited_transfers",
            FwriteColumn::Logical(t.iter().map(|x| Some(x.unlimited_transfers)).collect()),
        ),
        col(
            "allow_same_route_transfer",
            FwriteColumn::Logical(
                t.iter()
                    .map(|x| Some(x.allow_same_route_transfer))
                    .collect(),
            ),
        ),
        col(
            "use_route_fare",
            FwriteColumn::Logical(t.iter().map(|x| Some(x.use_route_fare)).collect()),
        ),
        col(
            "fare",
            FwriteColumn::Number(t.iter().map(|x| x.fare).collect()),
        ),
    ])?;
    let tr = &s.fares_per_transfer;
    let transfers = fwrite_csv(&[
        col("first_leg", text(tr.iter().map(|x| x.first_leg.clone()))),
        col("second_leg", text(tr.iter().map(|x| x.second_leg.clone()))),
        col(
            "fare",
            FwriteColumn::Number(tr.iter().map(|x| x.fare).collect()),
        ),
    ])?;
    let r = &s.fares_per_route;
    let routes = fwrite_csv(&[
        col("agency_id", text(r.iter().map(|x| x.agency_id.clone()))),
        col("agency_name", text(r.iter().map(|x| x.agency_name.clone()))),
        col("route_id", text(r.iter().map(|x| x.route_id.clone()))),
        col(
            "route_short_name",
            text(r.iter().map(|x| x.route_short_name.clone())),
        ),
        col(
            "route_long_name",
            text(r.iter().map(|x| x.route_long_name.clone())),
        ),
        col("mode", text(r.iter().map(|x| x.mode.clone()))),
        col(
            "route_fare",
            FwriteColumn::Number(r.iter().map(|x| x.route_fare).collect()),
        ),
        col("fare_type", text(r.iter().map(|x| x.fare_type.clone()))),
    ])?;
    let debug = fwrite_csv(&[
        col(
            "setting",
            text(["output_file", "trip_info"].map(|s| Some(s.to_string()))),
        ),
        col(
            "value",
            text([
                Some(s.debug_output_file.clone()),
                Some(s.debug_trip_info.clone()),
            ]),
        ),
    ])?;
    Ok(vec![global, types, transfers, routes, debug])
}

/// r5r's `write_fare_structure()`: the bytes of the `.zip` file. The five
/// CSV files in it are byte for byte those r5r writes (`fwrite` with `\n`
/// line ends, as on Linux and macOS); the archive deflates them, as r5r's,
/// but leaves them undated (see the module notes), so the `.zip` bytes
/// differ.
pub fn write_fare_structure(s: &FareStructure) -> Result<Vec<u8>, RouteError> {
    let csvs = fare_csvs(s)?;
    let files: Vec<(&str, &[u8])> = FILES
        .iter()
        .zip(&csvs)
        .map(|(n, c)| (*n, c.as_bytes()))
        .collect();
    Ok(zip_deflated(&files))
}

/// A CSV file as `fread` splits it: the header and the rows of fields,
/// unquoted fields stripped of surrounding spaces (`strip.white`), and
/// whether each field was quoted.
fn fread_rows(text: &str) -> Vec<Vec<(String, bool)>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    let push = |row: &mut Vec<(String, bool)>, field: &mut String, quoted: bool| {
        let f = if quoted {
            std::mem::take(field)
        } else {
            let t = field.trim_matches([' ', '\t']).to_string();
            field.clear();
            t
        };
        row.push((f, quoted));
    };
    while let Some(c) = chars.next() {
        any = true;
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.trim().is_empty() && !quoted => {
                field.clear();
                quoted = true;
                in_quotes = true;
            }
            ',' => {
                push(&mut row, &mut field, quoted);
                quoted = false;
            }
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                push(&mut row, &mut field, quoted);
                quoted = false;
                if !(row.len() == 1 && row[0].0.is_empty() && !row[0].1) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
                any = false;
            }
            _ => field.push(c),
        }
    }
    if any {
        push(&mut row, &mut field, quoted);
        if !(row.len() == 1 && row[0].0.is_empty() && !row[0].1) {
            rows.push(row);
        }
    }
    rows
}

/// One table read by `fread(file, select = ...)`.
struct Table {
    file: &'static str,
    header: Vec<String>,
    rows: Vec<Vec<(String, bool)>>,
}

impl Table {
    fn read(arch: &Archive, file: &'static str, latin1: bool) -> Result<Table, RouteError> {
        let e = arch
            .find(file)
            .ok_or_else(|| RouteError::Invalid(format!("the fare structure file has no {file}")))?;
        let bytes = arch
            .read(e)
            .map_err(|e| RouteError::Invalid(format!("{file}: {e}")))?;
        let text = if latin1 {
            bytes.iter().map(|&b| char::from(b)).collect()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        let mut rows = fread_rows(&text);
        if rows.is_empty() {
            return Err(RouteError::Invalid(format!("{file} is empty")));
        }
        let header = rows.remove(0).into_iter().map(|f| f.0).collect();
        Ok(Table { file, header, rows })
    }

    fn column(&self, name: &str) -> Result<Vec<(&str, bool)>, RouteError> {
        let k =
            self.header.iter().position(|h| h == name).ok_or_else(|| {
                RouteError::Invalid(format!("{} has no column {name}", self.file))
            })?;
        Ok(self
            .rows
            .iter()
            .map(|r| r.get(k).map_or(("", false), |(s, q)| (s.as_str(), *q)))
            .collect())
    }

    /// A character column: `NA` (unquoted) is missing, an empty field the
    /// empty string.
    fn text(&self, name: &str) -> Result<Vec<Option<String>>, RouteError> {
        Ok(self
            .column(name)?
            .into_iter()
            .map(|(s, q)| (q || s != "NA").then(|| s.to_string()))
            .collect())
    }

    fn number(&self, name: &str) -> Result<Vec<f64>, RouteError> {
        self.column(name)?
            .into_iter()
            .map(|(s, _)| {
                fread_number(s).ok_or_else(|| {
                    RouteError::Invalid(format!(
                        "{}: {name} value {s:?} is not a number",
                        self.file
                    ))
                })
            })
            .collect()
    }

    fn logical(&self, name: &str) -> Result<Vec<Option<bool>>, RouteError> {
        self.column(name)?
            .into_iter()
            .map(|(s, _)| match s {
                "TRUE" | "True" | "true" | "T" => Ok(Some(true)),
                "FALSE" | "False" | "false" | "F" => Ok(Some(false)),
                "" | "NA" => Ok(None),
                _ => Err(RouteError::Invalid(format!(
                    "{}: {name} value {s:?} is not TRUE or FALSE",
                    self.file
                ))),
            })
            .collect()
    }

    /// The `value` of the row whose `setting` is `name`.
    fn setting(&self, name: &str) -> Result<(String, bool), RouteError> {
        let settings = self.column("setting")?;
        let values = self.column("value")?;
        let hits: Vec<_> = settings
            .iter()
            .zip(&values)
            .filter(|((s, _), _)| *s == name)
            .map(|(_, (v, q))| (v.to_string(), *q))
            .collect();
        match hits.as_slice() {
            [one] => Ok(one.clone()),
            _ => Err(RouteError::Invalid(format!(
                "{} must have one row for {name}, it has {}",
                self.file,
                hits.len()
            ))),
        }
    }
}

/// A number as `fread` reads it (`NA` and an empty field are NaN).
fn fread_number(s: &str) -> Option<f64> {
    match s {
        "" | "NA" | "NaN" => Some(f64::NAN),
        "Inf" | "+Inf" | "inf" | "INF" => Some(f64::INFINITY),
        "-Inf" | "-inf" | "-INF" => Some(f64::NEG_INFINITY),
        _ => s.parse::<f64>().ok().filter(|x| x.is_finite()),
    }
}

/// R's `as.integer()` of a double: truncated toward zero, NaN for `NA`
/// or beyond the range of an R integer.
fn as_integer(x: f64) -> f64 {
    if x.is_nan() || x.abs() >= 2_147_483_648.0 || x.trunc() == f64::from(i32::MIN) {
        f64::NAN
    } else {
        x.trunc()
    }
}

/// r5r's `read_fare_structure()` of the bytes of a fare structure `.zip`
/// file: the global settings (the counts made integers as `as.integer()`
/// does: `Inf` becomes `NA`, NaN here), the three tables with the columns
/// r5r selects, and the debug settings. `encoding` is fread's: `"UTF-8"`
/// (the default) and `"unknown"` read the text as UTF-8, `"Latin-1"` as
/// ISO 8859-1.
pub fn read_fare_structure(bytes: &[u8], encoding: &str) -> Result<FareStructure, RouteError> {
    let latin1 = match encoding {
        "UTF-8" | "unknown" => false,
        "Latin-1" => true,
        _ => {
            return Err(RouteError::Invalid(format!(
                "encoding must be one of \"unknown\", \"UTF-8\" and \"Latin-1\", got {encoding:?}"
            )))
        }
    };
    let arch = Archive::from_bytes(bytes)
        .map_err(|e| RouteError::Invalid(format!("fare structure file: {e}")))?;
    let g = Table::read(&arch, FILES[0], latin1)?;
    // the value column is numeric when every value reads as a number
    let num = |name: &str| -> Result<f64, RouteError> {
        let (v, _) = g.setting(name)?;
        fread_number(&v)
            .ok_or_else(|| RouteError::Invalid(format!("{}: {name} {v:?} is not a number", g.file)))
    };
    let max_discounted_transfers = as_integer(num("max_discounted_transfers")?);
    let transfer_time_allowance = as_integer(num("transfer_time_allowance")?);
    let fare_cap = num("fare_cap")?;
    let t = Table::read(&arch, FILES[1], latin1)?;
    let types = t.text("type")?;
    let (unl, same, use_route) = (
        t.logical("unlimited_transfers")?,
        t.logical("allow_same_route_transfer")?,
        t.logical("use_route_fare")?,
    );
    let fares = t.number("fare")?;
    let mut fares_per_type = Vec::with_capacity(types.len());
    for k in 0..types.len() {
        let req = |v: Option<bool>, col: &str| {
            v.ok_or_else(|| {
                RouteError::Invalid(format!("{}: {col} is NA in row {}", t.file, k + 1))
            })
        };
        fares_per_type.push(FarePerType {
            fare_type: types[k].clone(),
            unlimited_transfers: req(unl[k], "unlimited_transfers")?,
            allow_same_route_transfer: req(same[k], "allow_same_route_transfer")?,
            use_route_fare: req(use_route[k], "use_route_fare")?,
            fare: fares[k],
        });
    }
    let tr = Table::read(&arch, FILES[2], latin1)?;
    let (a, b, f) = (
        tr.text("first_leg")?,
        tr.text("second_leg")?,
        tr.number("fare")?,
    );
    let fares_per_transfer = (0..a.len())
        .map(|k| FarePerTransfer {
            first_leg: a[k].clone(),
            second_leg: b[k].clone(),
            fare: f[k],
        })
        .collect();
    let r = Table::read(&arch, FILES[3], latin1)?;
    let cols = [
        r.text("agency_id")?,
        r.text("agency_name")?,
        r.text("route_id")?,
        r.text("route_short_name")?,
        r.text("route_long_name")?,
        r.text("mode")?,
    ];
    let route_fare = r.number("route_fare")?;
    let fare_type = r.text("fare_type")?;
    let fares_per_route = (0..route_fare.len())
        .map(|k| FarePerRoute {
            agency_id: cols[0][k].clone(),
            agency_name: cols[1][k].clone(),
            route_id: cols[2][k].clone(),
            route_short_name: cols[3][k].clone(),
            route_long_name: cols[4][k].clone(),
            mode: cols[5][k].clone(),
            route_fare: route_fare[k],
            fare_type: fare_type[k].clone(),
        })
        .collect();
    let d = Table::read(&arch, FILES[4], latin1)?;
    let debug = |name: &str| -> Result<String, RouteError> {
        let (v, q) = d.setting(name)?;
        if !q && v == "NA" {
            return Err(RouteError::Invalid(format!("{}: {name} is NA", d.file)));
        }
        Ok(v)
    };
    Ok(FareStructure {
        max_discounted_transfers,
        transfer_time_allowance,
        fare_cap,
        fares_per_type,
        fares_per_transfer,
        fares_per_route,
        debug_output_file: debug("output_file")?,
        debug_trip_info: debug("trip_info")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fwrite_numbers() {
        let f = |x: f64| {
            let mut s = String::new();
            fwrite_number(x, &mut s);
            s
        };
        assert_eq!(f(4.8), "4.8");
        assert_eq!(f(8.37), "8.37");
        assert_eq!(f(120.0), "120");
        assert_eq!(f(f64::INFINITY), "Inf");
        assert_eq!(f(1.0 / 3.0), "0.333333333333333");
        assert_eq!(f(1e-5), "1e-05");
        assert_eq!(f(1e20), "1e+20");
        assert_eq!(f(123456.0), "123456");
    }

    #[test]
    fn roundtrip() {
        let mut s = FareStructure::default();
        s.fares_per_type.push(FarePerType {
            fare_type: Some("BUS".into()),
            fare: 4.8,
            ..FarePerType::default()
        });
        s.fares_per_route.push(FarePerRoute {
            agency_id: Some("A".into()),
            agency_name: Some("Agência, \"x\"".into()),
            route_id: Some("1".into()),
            route_short_name: Some(String::new()),
            route_long_name: Some("L".into()),
            mode: Some("BUS".into()),
            route_fare: 4.8,
            fare_type: Some("BUS".into()),
        });
        let z = write_fare_structure(&s).unwrap();
        let back = read_fare_structure(&z, "UTF-8").unwrap();
        assert_eq!(back.fares_per_route, s.fares_per_route);
        assert_eq!(back.fares_per_type, s.fares_per_type);
        assert_eq!(back.max_discounted_transfers, 1.0);
        assert!(back.fare_cap.is_infinite());
        assert_eq!(back.debug_output_file, "");
        assert_eq!(back.debug_trip_info, "MODE");
    }

    #[test]
    fn archives_are_deflated() {
        let text = "a,b\n".repeat(500);
        let z = zip_deflated(&[("x.csv", text.as_bytes()), ("y.csv", b"")]);
        assert!(z.len() < 300, "{}", z.len());
        // method 8 in the local header and in the central directory
        assert_eq!(&z[8..10], &[8, 0]);
        let cd = z
            .windows(4)
            .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        assert_eq!(&z[cd + 10..cd + 12], &[8, 0]);
        let arch = Archive::from_bytes(&z[..]).unwrap();
        let read = |name: &str| arch.read(arch.find(name).unwrap()).unwrap();
        assert_eq!(read("x.csv"), text.as_bytes());
        assert_eq!(read("y.csv"), b"");
    }
}
