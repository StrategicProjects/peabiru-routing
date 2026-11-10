//! CSV reading for GTFS tables, one record at a time, with the rules of
//! javacsv (`com.csvreader.CsvReader` 2.0) as gtfs-lib configures it for R5:
//! comma delimiter, `"` text qualifier with doubled quotes as the escape,
//! whitespace trimming, empty lines skipped, and the "safety switch" limits.
//!
//! - A record ends at CR, LF or CRLF (outside quotes).
//! - A field is quoted only when `"` is its first character after leading
//!   spaces and tabs. Inside quotes, `""` is a literal quote and line breaks
//!   are kept; after the closing quote everything up to the next delimiter
//!   or end of record is dropped. A quote anywhere else is a literal
//!   character. A quote left open at end of file ends the field there.
//! - Unquoted fields lose their leading and trailing spaces and tabs.
//! - A line with no characters at all is skipped; a line with only
//!   whitespace is a record with one empty field.
//! - A field longer than 100,000 characters, or a record of more than
//!   100,000 fields, is an error.
//! - A UTF-8 byte-order mark at the start is ignored.
//!
//! Records are handed to a callback and not kept, so memory stays
//! proportional to the typed rows the caller builds.

use std::fmt;

/// javacsv's `SafetySwitch` bounds: longest field (in UTF-16 code units, the
/// length of a Java string) and most fields in a record.
pub const MAX_FIELD_LEN: usize = 100_000;
pub const MAX_COLUMNS: usize = 100_000;

/// Column names of a table.
pub struct Header {
    names: Vec<String>,
}

impl Header {
    /// Index of a column, if present.
    pub fn col(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|h| h == name)
    }

    #[cfg(test)]
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

/// One record.
pub struct Row<'a> {
    fields: &'a [String],
}

impl Row<'_> {
    /// Field at column `c`, trimmed; empty when the record is short or the
    /// column is absent.
    pub fn get(&self, c: Option<usize>) -> &str {
        c.and_then(|c| self.fields.get(c)).map_or("", |s| s.trim())
    }
}

/// A file that javacsv refuses to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvError {
    /// Field `column` (1-based) of the record starting at `line` is longer
    /// than [`MAX_FIELD_LEN`]; usually an unbalanced quote.
    FieldTooLong { line: usize, column: usize },
    /// The record starting at `line` has more than [`MAX_COLUMNS`] fields.
    TooManyColumns { line: usize },
}

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CsvError::FieldTooLong { line, column } => write!(
                f,
                "line {line}: field {column} is longer than {MAX_FIELD_LEN} characters \
                 (an opening quote without a closing one?)"
            ),
            CsvError::TooManyColumns { line } => {
                write!(f, "line {line}: more than {MAX_COLUMNS} fields")
            }
        }
    }
}

impl std::error::Error for CsvError {}

/// Error of [`for_each_row`]: the file itself, or the callback.
#[derive(Debug)]
pub enum ReadError<E> {
    Csv(CsvError),
    Callback(E),
}

#[inline]
fn is_blank(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

#[inline]
fn ends_field(c: u8) -> bool {
    c == b',' || c == b'\r' || c == b'\n'
}

/// Length of `s` as a Java string (UTF-16 code units).
fn java_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// Call `f(header, line, row)` for every record after the header, where
/// `line` is the 1-based line of the file on which the record starts.
/// Returns the header, or `None` for input without any record.
pub fn for_each_row<E>(
    bytes: &[u8],
    mut f: impl FnMut(&Header, usize, &Row) -> Result<(), E>,
) -> Result<Option<Header>, ReadError<E>> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let n = bytes.len();
    let mut header: Option<Header> = None;
    // Field buffers are reused across records: `fields[..used]` is the record.
    let mut fields: Vec<String> = Vec::new();
    let mut raw: Vec<u8> = Vec::new();
    let mut line = 1usize;
    let mut i = 0usize;
    while i < n {
        // Empty line: skipped.
        match bytes[i] {
            b'\n' => {
                i += 1;
                line += 1;
                continue;
            }
            b'\r' => {
                i += 1;
                if bytes.get(i) == Some(&b'\n') {
                    i += 1;
                }
                line += 1;
                continue;
            }
            _ => {}
        }
        let record_line = line;
        let mut used = 0usize;
        loop {
            while i < n && is_blank(bytes[i]) {
                i += 1;
            }
            raw.clear();
            if i < n && bytes[i] == b'"' {
                i += 1;
                while i < n {
                    let c = bytes[i];
                    if c == b'"' {
                        if bytes.get(i + 1) == Some(&b'"') {
                            raw.push(b'"');
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    if c == b'\n' || (c == b'\r' && bytes.get(i + 1) != Some(&b'\n')) {
                        line += 1;
                    }
                    raw.push(c);
                    i += 1;
                }
                // Text between the closing quote and the delimiter is dropped.
                while i < n && !ends_field(bytes[i]) {
                    i += 1;
                }
            } else {
                let start = i;
                while i < n && !ends_field(bytes[i]) {
                    i += 1;
                }
                let mut end = i;
                while end > start && is_blank(bytes[end - 1]) {
                    end -= 1;
                }
                raw.extend_from_slice(&bytes[start..end]);
            }
            if used == MAX_COLUMNS {
                return Err(ReadError::Csv(CsvError::TooManyColumns {
                    line: record_line,
                }));
            }
            if used == fields.len() {
                fields.push(String::new());
            }
            let field = &mut fields[used];
            field.clear();
            field.push_str(&String::from_utf8_lossy(&raw));
            used += 1;
            // UTF-16 length never exceeds the UTF-8 length.
            if field.len() > MAX_FIELD_LEN && java_len(field) > MAX_FIELD_LEN {
                return Err(ReadError::Csv(CsvError::FieldTooLong {
                    line: record_line,
                    column: used,
                }));
            }
            if i < n && bytes[i] == b',' {
                i += 1;
                continue;
            }
            if i < n {
                if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
                i += 1;
                line += 1;
            }
            break;
        }
        let record = &fields[..used];
        match &header {
            None => {
                header = Some(Header {
                    names: record.iter().map(|s| s.trim().to_string()).collect(),
                });
            }
            Some(h) => f(h, record_line, &Row { fields: record }).map_err(ReadError::Callback)?,
        }
    }
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    type Rows = Vec<(usize, Vec<String>)>;

    /// Header and records as javacsv returns them (no trimming by `get`).
    fn parse(bytes: &[u8]) -> Result<(Vec<String>, Rows), CsvError> {
        let mut rows = Vec::new();
        let h = for_each_row::<()>(bytes, |_, line, r| {
            rows.push((line, r.fields.to_vec()));
            Ok(())
        })
        .map_err(|e| match e {
            ReadError::Csv(e) => e,
            ReadError::Callback(()) => unreachable!(),
        })?;
        Ok((h.map(|h| h.names().to_vec()).unwrap_or_default(), rows))
    }

    fn records(bytes: &[u8]) -> Vec<Vec<String>> {
        parse(bytes).unwrap().1.into_iter().map(|r| r.1).collect()
    }

    #[test]
    fn quotes_bom_and_line_endings() {
        let (h, rows) =
            parse(b"\xef\xbb\xbfa,b ,c\r\n1,\"x, \"\"y\"\"\",\r\n\r\n2,\"\",z\n").unwrap();
        assert_eq!(h, ["a", "b", "c"]);
        assert_eq!(rows[0].1, ["1", "x, \"y\"", ""]);
        assert_eq!(rows[1].1, ["2", "", "z"]);
    }

    #[test]
    fn quoted_newlines_short_rows_and_no_trailing_newline() {
        assert_eq!(
            records(b"a,b\n\"line\nbreak\",2\nonly"),
            [vec!["line\nbreak", "2"], vec!["only"]]
        );
    }

    /// The expected values below are what javacsv 2.0 returns for the same
    /// input, read with `new CsvReader(stream, ',', UTF-8)` as gtfs-lib does.
    #[test]
    fn quote_rules_match_javacsv() {
        // A quote that does not open the field is literal.
        assert_eq!(
            records(b"a,b,c\nA,Rua 7\" Norte,1\nB,x,2\n"),
            [vec!["A", "Rua 7\" Norte", "1"], vec!["B", "x", "2"]]
        );
        assert_eq!(
            records(b"a,b\nA,x  \"ab\"  ,g\n"),
            [["A", "x  \"ab\"", "g"]]
        );
        assert_eq!(records(b"a,b\nA,x\\\"y\n"), [["A", "x\\\"y"]]);
        // Text after the closing quote is dropped up to the delimiter.
        assert_eq!(records(b"a,b\nA,\"ab\"cd,3\n"), [["A", "ab", "3"]]);
        assert_eq!(
            records(b"a,b\nA,\"ab\"c\"d,e\"f,g\n"),
            [["A", "ab", "e\"f", "g"]]
        );
        assert_eq!(
            records(b"a,b\nA,\" x\"\" y \"  z\nB"),
            [vec!["A", " x\" y "], vec!["B"]]
        );
        // Whitespace before the opening quote is skipped; inside it is kept.
        assert_eq!(records(b"a,b\nA,  \"ab\"  ,g\n"), [["A", "ab", "g"]]);
        assert_eq!(
            records(b"a,b\n \" x \" , y \nB,2"),
            [[" x ", "y"], ["B", "2"]]
        );
        // A quote open at end of file ends the field there.
        assert_eq!(
            records(b"a,b\nA,\"unterminated\nB,2\n"),
            [["A", "unterminated\nB,2\n"]]
        );
        assert_eq!(records(b"a,b\nA,\""), [["A", ""]]);
        assert_eq!(records(b"a,b\nA,\"x\"\""), [["A", "x\""]]);
    }

    #[test]
    fn line_endings_and_blank_lines_match_javacsv() {
        // CR alone ends a record.
        assert_eq!(records(b"a,b\rA,1\rB,2\r"), [["A", "1"], ["B", "2"]]);
        assert_eq!(records(b"a,b\r\rB,2\r\n\nC,3"), [["B", "2"], ["C", "3"]]);
        // CR inside quotes is data.
        assert_eq!(
            records(b"a,b\nA,\"a\rb\"\nB,2"),
            [["A", "a\rb"], ["B", "2"]]
        );
        // Only spaces and tabs are trimmed; whitespace-only lines are records.
        assert_eq!(
            records(b"a,b\n\tA\t,\x0cB\x0c\n   \n,\n"),
            [vec!["A", "\x0cB\x0c"], vec![""], vec!["", ""]]
        );
        // Blank lines before the header are skipped; no record means no header.
        let (h, rows) = parse(b"\n\r\na,b\nA,1").unwrap();
        assert_eq!((h, rows.len()), (vec!["a".to_string(), "b".into()], 1));
        assert!(for_each_row::<()>(b"\n\r\n", |_, _, _| Ok(()))
            .unwrap()
            .is_none());
    }

    #[test]
    fn records_carry_their_file_line() {
        let (_, rows) = parse(b"a,b\n\n1,\"x\r\ny\"\r\n2,z\r3,w\n").unwrap();
        let lines: Vec<usize> = rows.iter().map(|r| r.0).collect();
        assert_eq!(lines, [3, 5, 6]);
    }

    #[test]
    fn safety_switch_limits() {
        let mut ok = b"a,b\nA,\"".to_vec();
        ok.extend(std::iter::repeat(b'x').take(MAX_FIELD_LEN));
        assert!(parse(&ok).is_ok());
        ok.push(b'x');
        assert_eq!(
            parse(&ok).unwrap_err(),
            CsvError::FieldTooLong { line: 2, column: 2 }
        );
        // Characters outside the BMP count twice, as in Java.
        let mut wide = b"a\n".to_vec();
        for _ in 0..MAX_FIELD_LEN / 2 {
            wide.extend("\u{1F600}".as_bytes());
        }
        assert!(parse(&wide).is_ok());
        wide.extend(b"x");
        assert!(parse(&wide).is_err());
        let mut cols = b"a\n".to_vec();
        cols.extend(std::iter::repeat(b',').take(MAX_COLUMNS - 1));
        assert!(parse(&cols).is_ok());
        cols.push(b',');
        assert_eq!(
            parse(&cols).unwrap_err(),
            CsvError::TooManyColumns { line: 2 }
        );
    }

    #[test]
    fn errors_stop_the_scan() {
        let mut seen = 0;
        let r = for_each_row(b"a\n1\n2\n3\n", |_, line, _| {
            seen += 1;
            if line == 3 {
                Err("stop")
            } else {
                Ok(())
            }
        });
        assert!(matches!(r, Err(ReadError::Callback("stop"))));
        assert_eq!(seen, 2);
    }
}
