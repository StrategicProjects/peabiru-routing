//! RFC 4180 CSV reading for GTFS tables, one record at a time: header row,
//! quoted fields with doubled quotes, CRLF or LF, an optional UTF-8
//! byte-order mark. Records are handed to a callback and not kept, so memory
//! stays proportional to the typed rows the caller builds.

/// Column names of a table.
pub struct Header {
    names: Vec<String>,
}

impl Header {
    /// Index of a column, if present.
    pub fn col(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|h| h == name)
    }

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

/// Call `f(header, record_number, row)` for every record after the header
/// (`record_number` is 1-based, as a spreadsheet row minus the header).
/// Blank lines are skipped. Returns the header, or `None` for empty input.
pub fn for_each_row<E>(
    bytes: &[u8],
    mut f: impl FnMut(&Header, usize, &Row) -> Result<(), E>,
) -> Result<Option<Header>, E> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let mut header: Option<Header> = None;
    let mut fields: Vec<String> = Vec::new();
    let mut field: Vec<u8> = Vec::new();
    let mut quoted = false;
    let mut any = false;
    let mut n = 0usize;
    let mut emit = |fields: &mut Vec<String>, header: &mut Option<Header>| -> Result<(), E> {
        match header {
            None => {
                *header = Some(Header {
                    names: fields.iter().map(|s| s.trim().to_string()).collect(),
                });
            }
            Some(h) => {
                n += 1;
                f(h, n, &Row { fields })?;
            }
        }
        fields.clear();
        Ok(())
    };
    let take = |field: &mut Vec<u8>| {
        let s = String::from_utf8_lossy(field).into_owned();
        field.clear();
        s
    };
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        i += 1;
        if quoted {
            if c == b'"' {
                if bytes.get(i) == Some(&b'"') {
                    field.push(b'"');
                    i += 1;
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            b'"' => {
                quoted = true;
                any = true;
            }
            b',' => {
                fields.push(take(&mut field));
                any = true;
            }
            b'\r' => {}
            b'\n' => {
                if any || !field.is_empty() {
                    fields.push(take(&mut field));
                    emit(&mut fields, &mut header)?;
                }
                any = false;
            }
            _ => {
                field.push(c);
                any = true;
            }
        }
    }
    if any || !field.is_empty() {
        fields.push(take(&mut field));
        emit(&mut fields, &mut header)?;
    }
    Ok(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(bytes: &[u8]) -> (Vec<String>, Vec<Vec<String>>) {
        let mut rows = Vec::new();
        let h = for_each_row::<()>(bytes, |h, _, r| {
            rows.push(
                (0..h.names().len())
                    .map(|c| r.get(Some(c)).to_string())
                    .collect(),
            );
            Ok(())
        })
        .unwrap();
        (h.map(|h| h.names().to_vec()).unwrap_or_default(), rows)
    }

    #[test]
    fn quotes_bom_and_line_endings() {
        let (h, rows) = collect(b"\xef\xbb\xbfa,b ,c\r\n1,\"x, \"\"y\"\"\",\r\n\r\n2,\"\",z\n");
        assert_eq!(h, ["a", "b", "c"]);
        assert_eq!(rows, [["1", "x, \"y\"", ""], ["2", "", "z"]]);
    }

    #[test]
    fn quoted_newlines_short_rows_and_no_trailing_newline() {
        let (_, rows) = collect(b"a,b\n\"line\nbreak\",2\nonly");
        assert_eq!(rows, [["line\nbreak", "2"], ["only", ""]]);
    }

    #[test]
    fn errors_stop_the_scan() {
        let mut seen = 0;
        let r = for_each_row(b"a\n1\n2\n3\n", |_, n, _| {
            seen += 1;
            if n == 2 {
                Err("stop")
            } else {
                Ok(())
            }
        });
        assert!(r.is_err());
        assert_eq!(seen, 2);
    }
}
