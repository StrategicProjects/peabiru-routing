//! RFC 4180 CSV reading for GTFS tables: header row, quoted fields with
//! doubled quotes, CRLF or LF, an optional UTF-8 byte-order mark.

/// A parsed table: column names and rows of fields.
pub struct Table {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// Index of a column, if present.
    pub fn col(&self, name: &str) -> Option<usize> {
        self.header.iter().position(|h| h == name)
    }

    /// Field `c` of row `r`, trimmed; empty when the row is short or the
    /// column is absent.
    pub fn get<'a>(&'a self, row: &'a [String], c: Option<usize>) -> &'a str {
        c.and_then(|c| row.get(c)).map_or("", |s| s.trim())
    }
}

/// Parse CSV text. Blank lines are skipped.
pub fn parse(bytes: &[u8]) -> Table {
    let text = String::from_utf8_lossy(bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut records = Vec::new();
    let mut field = String::new();
    let mut record: Vec<String> = Vec::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => {
                quoted = true;
                any = true;
            }
            ',' => {
                record.push(std::mem::take(&mut field));
                any = true;
            }
            '\r' => {}
            '\n' => {
                if any || !field.is_empty() {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
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
        record.push(field);
        records.push(record);
    }
    let mut it = records.into_iter();
    let header = it
        .next()
        .unwrap_or_default()
        .into_iter()
        .map(|h| h.trim().to_string())
        .collect();
    Table {
        header,
        rows: it.collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_bom_and_line_endings() {
        let t = parse(b"\xef\xbb\xbfa,b ,c\r\n1,\"x, \"\"y\"\"\",\r\n\r\n2,\"\",z\n");
        assert_eq!(t.header, ["a", "b", "c"]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0], ["1", "x, \"y\"", ""]);
        assert_eq!(t.get(&t.rows[1], t.col("c")), "z");
        assert_eq!(t.get(&t.rows[1], t.col("missing")), "");
    }

    #[test]
    fn quoted_newlines_and_no_trailing_newline() {
        let t = parse(b"a,b\n\"line\nbreak\",2");
        assert_eq!(
            t.rows,
            vec![vec!["line\nbreak".to_string(), "2".to_string()]]
        );
    }
}
