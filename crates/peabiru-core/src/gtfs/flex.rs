//! GTFS-Flex zones: `locations.geojson`, read as gtfs-lib's
//! `FlexLocationStreamer` reads it (as bundled in R5 7.6), for the errors
//! gtfs-lib records and the zone ids that stop times and location groups
//! may refer to. The zones themselves are never routed on: R5 builds
//! on-demand services from them only for requests with the `ON_DEMAND`
//! flag, which r5r never sets.
//!
//! gtfs-lib streams the file with Jackson and expects one exact shape: a
//! `FeatureCollection` object whose only members are `type` and
//! `features`; features whose only members are `type` (`"Feature"`), `id`
//! (a string), `properties` (an object of strings) and `geometry`; and
//! geometries whose only members are `type` and `coordinates`. Anything
//! else, malformed JSON included, throws (`Failed to parse GeoJSON.`) and
//! the feed fails to load; a geometry that is not a `Polygon` is an
//! `UnsupportedGeometryException`, after which gtfs-lib keeps no zone and
//! records one `UnsupportedFlexError`.

use std::collections::HashSet;

/// What reading `locations.geojson` gives.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Locations {
    /// The ids of the zones.
    Zones(HashSet<String>),
    /// A geometry gtfs-lib does not support: no zone is kept.
    Unsupported,
}

/// Why reading fails: gtfs-lib throws (`Fails`) or peabiru cannot tell
/// what Jackson would do (`Unread`).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LocationsError {
    Fails(String),
    Unread(String),
}

/// A JSON token, as Jackson's streaming parser returns them.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    StartObject,
    EndObject,
    StartArray,
    EndArray,
    FieldName(String),
    Str(String),
    Num(f64),
    Literal,
}

/// Where the parser is: in an object (`expect_name`: before a member) or
/// an array (`first`: no element yet).
#[derive(Clone, Copy)]
enum Ctx {
    Object { expect_name: bool, first: bool },
    Array { first: bool },
}

/// A lazy JSON tokenizer: it reads only as far as the tokens asked for,
/// as Jackson does, so text after the root object is never looked at.
struct Lexer<'a> {
    b: &'a [u8],
    i: usize,
    stack: Vec<Ctx>,
    done: bool,
}

/// Jackson's default limit on nesting (`StreamReadConstraints`).
const MAX_DEPTH: usize = 1000;

impl<'a> Lexer<'a> {
    fn new(b: &'a [u8]) -> Self {
        let b = b.strip_prefix(b"\xef\xbb\xbf").unwrap_or(b);
        Lexer {
            b,
            i: 0,
            stack: Vec::new(),
            done: false,
        }
    }

    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\r' | b'\n') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what} at byte {}", self.i))
    }

    /// The next token, `None` at the end of the input (or after the root
    /// value).
    fn next(&mut self) -> Result<Option<Tok>, String> {
        if self.done {
            return Ok(None);
        }
        self.ws();
        match self.stack.last().copied() {
            None => {}
            Some(Ctx::Object { expect_name, first }) => {
                if expect_name {
                    if self.peek() == Some(b'}') && first {
                        self.i += 1;
                        return Ok(Some(self.close()));
                    }
                    if !first {
                        match self.peek() {
                            Some(b',') => {
                                self.i += 1;
                                self.ws();
                            }
                            Some(b'}') => {
                                self.i += 1;
                                return Ok(Some(self.close()));
                            }
                            _ => return self.err("expected ',' or '}'"),
                        }
                    }
                    if self.peek() != Some(b'"') {
                        return self.err("expected a field name");
                    }
                    let name = self.string()?;
                    self.ws();
                    if self.peek() != Some(b':') {
                        return self.err("expected ':'");
                    }
                    self.i += 1;
                    if let Some(Ctx::Object { expect_name, first }) = self.stack.last_mut() {
                        *expect_name = false;
                        *first = false;
                    }
                    return Ok(Some(Tok::FieldName(name)));
                }
                if let Some(Ctx::Object { expect_name, .. }) = self.stack.last_mut() {
                    *expect_name = true;
                }
            }
            Some(Ctx::Array { first }) => {
                if self.peek() == Some(b']') {
                    self.i += 1;
                    return Ok(Some(self.close()));
                }
                if !first {
                    if self.peek() != Some(b',') {
                        return self.err("expected ',' or ']'");
                    }
                    self.i += 1;
                    self.ws();
                    if self.peek() == Some(b']') {
                        return self.err("trailing comma");
                    }
                }
                if let Some(Ctx::Array { first }) = self.stack.last_mut() {
                    *first = false;
                }
            }
        }
        self.value().map(Some)
    }

    fn close(&mut self) -> Tok {
        let ctx = self.stack.pop();
        if self.stack.is_empty() {
            self.done = true;
        }
        match ctx {
            Some(Ctx::Object { .. }) => Tok::EndObject,
            _ => Tok::EndArray,
        }
    }

    /// A value starting at the current byte.
    fn value(&mut self) -> Result<Tok, String> {
        let root = self.stack.is_empty();
        let tok = match self.peek() {
            None => return self.err("unexpected end of input"),
            Some(b'{') | Some(b'[') => {
                if self.stack.len() >= MAX_DEPTH {
                    return self.err("nesting too deep");
                }
                let object = self.peek() == Some(b'{');
                self.i += 1;
                self.stack.push(if object {
                    Ctx::Object {
                        expect_name: true,
                        first: true,
                    }
                } else {
                    Ctx::Array { first: true }
                });
                return Ok(if object {
                    Tok::StartObject
                } else {
                    Tok::StartArray
                });
            }
            Some(b'"') => Tok::Str(self.string()?),
            Some(b'-' | b'0'..=b'9') => Tok::Num(self.number()?),
            Some(b't') => self.literal(b"true")?,
            Some(b'f') => self.literal(b"false")?,
            Some(b'n') => self.literal(b"null")?,
            Some(_) => return self.err("unexpected character"),
        };
        if root {
            self.done = true;
        }
        Ok(tok)
    }

    fn literal(&mut self, word: &[u8]) -> Result<Tok, String> {
        if self.b[self.i..].starts_with(word) {
            self.i += word.len();
            // Jackson reads the rest of an identifier as part of the token
            if self
                .peek()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                return self.err("unrecognized token");
            }
            Ok(Tok::Literal)
        } else {
            self.err("unrecognized token")
        }
    }

    /// `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`, as a double.
    fn number(&mut self) -> Result<f64, String> {
        let start = self.i;
        let digits = |l: &mut Self| {
            let s = l.i;
            while l.peek().is_some_and(|c| c.is_ascii_digit()) {
                l.i += 1;
            }
            l.i - s
        };
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        let int_start = self.i;
        let n = digits(self);
        if n == 0 {
            return self.err("expected a digit");
        }
        if n > 1 && self.b[int_start] == b'0' {
            return self.err("leading zeroes are not allowed");
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            if digits(self) == 0 {
                return self.err("expected a digit after the decimal point");
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if digits(self) == 0 {
                return self.err("expected a digit in the exponent");
            }
        }
        let text = std::str::from_utf8(&self.b[start..self.i]).expect("ASCII");
        text.parse::<f64>()
            .or_else(|_| self.err("number out of range"))
    }

    /// A string starting at the opening quote.
    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(c) = self.peek() else {
                return self.err("unterminated string");
            };
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let Some(e) = self.peek() else {
                        return self.err("unterminated string");
                    };
                    self.i += 1;
                    match e {
                        b'"' | b'\\' | b'/' => out.push(e),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let mut unit = self.hex4()?;
                            let ch = if (0xd800..0xdc00).contains(&unit)
                                && self.b[self.i..].starts_with(b"\\u")
                            {
                                self.i += 2;
                                let low = self.hex4()?;
                                if (0xdc00..0xe000).contains(&low) {
                                    char::from_u32(
                                        0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00),
                                    )
                                } else {
                                    // a lone surrogate, then another unit
                                    out.extend_from_slice("\u{fffd}".as_bytes());
                                    unit = low;
                                    char::from_u32(unit)
                                }
                            } else {
                                char::from_u32(unit)
                            };
                            // Java keeps lone surrogates; they never match an
                            // id read from a CSV file
                            let ch = ch.unwrap_or('\u{fffd}');
                            let mut buf = [0; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return self.err("unrecognized escape"),
                    }
                }
                0..=0x1f => return self.err("unescaped control character"),
                _ => out.push(c),
            }
        }
        String::from_utf8(out).or_else(|_| self.err("invalid UTF-8"))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let Some(h) = self.b.get(self.i..self.i + 4) else {
            return self.err("unterminated escape");
        };
        let s = std::str::from_utf8(h).unwrap_or("x");
        let v = u32::from_str_radix(s, 16).or_else(|_| self.err("invalid escape"))?;
        if !h.iter().all(u8::is_ascii_hexdigit) {
            return self.err("invalid escape");
        }
        self.i += 4;
        Ok(v)
    }
}

/// Failures of the streamer: an `IllegalArgumentException` or any other
/// exception (`Fail`), or an `UnsupportedGeometryException`.
enum Stop {
    Fail(String),
    Unsupported,
}

impl From<String> for Stop {
    fn from(s: String) -> Self {
        Stop::Fail(s)
    }
}

/// The streamer, holding the current token (`jp.currentToken()`).
struct Streamer<'a> {
    lx: Lexer<'a>,
    cur: Option<Tok>,
}

impl Streamer<'_> {
    fn next(&mut self) -> Result<&Option<Tok>, Stop> {
        self.cur = self.lx.next()?;
        Ok(&self.cur)
    }

    fn expect_next(&mut self, t: Tok) -> Result<(), Stop> {
        if *self.next()? != Some(t) {
            return Err(Stop::Fail("unexpected token".into()));
        }
        Ok(())
    }

    fn expect_current_field_name(&self) -> Result<String, Stop> {
        match &self.cur {
            Some(Tok::FieldName(n)) => Ok(n.clone()),
            _ => Err(Stop::Fail("expected a field name".into())),
        }
    }

    fn expect_next_string(&mut self) -> Result<String, Stop> {
        match self.next()? {
            Some(Tok::Str(s)) => Ok(s.clone()),
            _ => Err(Stop::Fail("expected a string".into())),
        }
    }

    /// `while (jp.nextToken() != end)`: whether a token other than `end`
    /// came (the end of input is such a token, as `null` in Java).
    fn more(&mut self, end: Tok) -> Result<bool, Stop> {
        Ok(*self.next()? != Some(end))
    }

    /// `GeoJsonStreamer.stream`: the zone ids.
    fn stream(&mut self) -> Result<HashSet<String>, Stop> {
        let mut ids = HashSet::new();
        self.expect_next(Tok::StartObject)?;
        while self.more(Tok::EndObject)? {
            match self.expect_current_field_name()?.as_str() {
                "type" => {
                    if self.expect_next_string()? != "FeatureCollection" {
                        return Err(Stop::Fail("not a FeatureCollection".into()));
                    }
                }
                "features" => {
                    self.expect_next(Tok::StartArray)?;
                    while self.more(Tok::EndArray)? {
                        self.read_one_feature(&mut ids)?;
                    }
                }
                _ => {} // the value is read as the next field name
            }
        }
        Ok(ids)
    }

    /// `FlexLocationStreamer.readOneFeature`.
    fn read_one_feature(&mut self, ids: &mut HashSet<String>) -> Result<(), Stop> {
        if self.cur != Some(Tok::StartObject) {
            return Err(Stop::Fail("a feature is not an object".into()));
        }
        let mut id = None;
        let mut polygon = None;
        while self.more(Tok::EndObject)? {
            match self.expect_current_field_name()?.as_str() {
                "type" => {
                    if self.expect_next_string()? != "Feature" {
                        return Err(Stop::Fail("not a Feature".into()));
                    }
                }
                "id" => id = Some(self.expect_next_string()?),
                "properties" => {
                    self.expect_next(Tok::StartObject)?;
                    while self.more(Tok::EndObject)? {
                        self.expect_current_field_name()?;
                        self.expect_next_string()?;
                    }
                }
                "geometry" => polygon = self.stream_one_polygon()?,
                _ => {}
            }
        }
        // cPolygon.validate(): JTS's LinearRing needs 0 or at least 4
        // points (its validity is computed and ignored)
        let Some(rings) = polygon else {
            return Err(Stop::Fail("a feature without coordinates".into()));
        };
        if rings.iter().any(|&n| n < 4) {
            return Err(Stop::Fail("a ring of fewer than four points".into()));
        }
        // the zones go to a MapDB map, which refuses a null key
        let Some(id) = id else {
            return Err(Stop::Fail("a feature without an id".into()));
        };
        ids.insert(id);
        Ok(())
    }

    /// `streamOnePolygon`: the number of points of each ring, `None`
    /// without `coordinates`.
    fn stream_one_polygon(&mut self) -> Result<Option<Vec<usize>>, Stop> {
        let mut polygon = None;
        self.expect_next(Tok::StartObject)?;
        while self.more(Tok::EndObject)? {
            match self.expect_current_field_name()?.as_str() {
                "coordinates" => polygon = Some(self.stream_rings()?),
                "type" => {
                    if self.expect_next_string()? != "Polygon" {
                        return Err(Stop::Unsupported);
                    }
                }
                _ => return Err(Stop::Fail("unrecognized field in a polygon".into())),
            }
        }
        Ok(polygon)
    }

    /// `CPolygon.fromRings(streamPositionArrays())`.
    fn stream_rings(&mut self) -> Result<Vec<usize>, Stop> {
        let mut rings = Vec::new();
        self.expect_next(Tok::StartArray)?;
        while self.more(Tok::EndArray)? {
            let ring = self.stream_one_position_array()?;
            rings.push(ring);
        }
        if rings.is_empty() {
            return Err(Stop::Fail("a polygon without rings".into()));
        }
        // CLinearRing: at least two points, the first and the last equal
        let mut points = Vec::with_capacity(rings.len());
        for ring in rings {
            let n = ring.len();
            if n < 2 || ring[0] != ring[n - 1] {
                return Err(Stop::Fail("a ring that is not closed".into()));
            }
            points.push(n);
        }
        Ok(points)
    }

    /// `streamOnePositionArray`: the `(x, y)` positions of a ring.
    fn stream_one_position_array(&mut self) -> Result<Vec<(f64, f64)>, Stop> {
        if self.cur != Some(Tok::StartArray) {
            return Err(Stop::Fail("a ring is not an array".into()));
        }
        let mut out = Vec::new();
        while self.more(Tok::EndArray)? {
            // an IllegalArgumentException here is an unsupported geometry
            let mut position = || -> Result<Option<(f64, f64)>, String> {
                if self.cur != Some(Tok::StartArray) {
                    return Ok(None);
                }
                let Some(Tok::Num(x)) = self.lx.next()? else {
                    return Ok(None);
                };
                let Some(Tok::Num(y)) = self.lx.next()? else {
                    return Ok(None);
                };
                Ok((self.lx.next()? == Some(Tok::EndArray)).then_some((x, y)))
            };
            match position()? {
                Some(p) => out.push(p),
                None => return Err(Stop::Unsupported),
            }
            self.cur = Some(Tok::EndArray);
        }
        Ok(out)
    }
}

/// The zones of `locations.geojson`, read as gtfs-lib reads them.
pub(crate) fn read_locations(bytes: &[u8]) -> Result<Locations, LocationsError> {
    // Jackson detects UTF-16 and UTF-32 from the first bytes; peabiru
    // reads UTF-8 only
    let head = &bytes[..bytes.len().min(4)];
    if head.starts_with(b"\xfe\xff") || head.starts_with(b"\xff\xfe") || head.contains(&0) {
        return Err(LocationsError::Unread(
            "locations.geojson is not in UTF-8, which is the only encoding of it peabiru reads"
                .into(),
        ));
    }
    let mut s = Streamer {
        lx: Lexer::new(bytes),
        cur: None,
    };
    match s.stream() {
        Ok(ids) => Ok(Locations::Zones(ids)),
        Err(Stop::Unsupported) => Ok(Locations::Unsupported),
        Err(Stop::Fail(why)) => Err(LocationsError::Fails(format!(
            "locations.geojson is not GeoJSON of the shape gtfs-lib reads ({why}), and gtfs-lib \
             fails reading it (\"Failed to parse GeoJSON.\")"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(s: &str) -> Result<Locations, LocationsError> {
        read_locations(s.as_bytes())
    }

    fn zones(ids: &[&str]) -> Result<Locations, LocationsError> {
        Ok(Locations::Zones(
            ids.iter().map(|s| s.to_string()).collect(),
        ))
    }

    const SQUARE: &str = r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]}"#;

    #[test]
    fn reads_polygons() {
        let doc = format!(
            r#"{{"type":"FeatureCollection","features":[
                {{"type":"Feature","id":"z1","properties":{{"stop_name":"A"}},"geometry":{SQUARE}}},
                {{"id":"z2","geometry":{SQUARE}}}]}} trailing text is never read"#
        );
        assert_eq!(read(&doc), zones(&["z1", "z2"]));
        assert_eq!(read(r#"{"features":[]}"#), zones(&[]));
        // a hole
        let holed = r#"{"features":[{"id":"h","geometry":{"coordinates":[
            [[0,0],[3,0],[3,3],[0,0]],[[1,1],[2,1],[2,2],[1,1]]],"type":"Polygon"}}]}"#;
        assert_eq!(read(holed), zones(&["h"]));
    }

    #[test]
    fn unsupported_geometries() {
        let multi =
            r#"{"features":[{"id":"m","geometry":{"type":"MultiPolygon","coordinates":[]}}]}"#;
        assert_eq!(read(multi), Ok(Locations::Unsupported));
        // coordinates first: found to be deeper than a polygon's
        let deep =
            r#"{"features":[{"id":"m","geometry":{"coordinates":[[[[0,0],[1,0],[1,1],[0,0]]]]}}]}"#;
        assert_eq!(read(deep), Ok(Locations::Unsupported));
        let three_d = r#"{"features":[{"id":"m","geometry":{"coordinates":[[[0,0,5],[1,0,5],[1,1,5],[0,0,5]]]}}]}"#;
        assert_eq!(read(three_d), Ok(Locations::Unsupported));
    }

    #[test]
    fn shapes_gtfs_lib_fails_on() {
        for bad in [
            "",
            "[]",
            r#"{"type":"Collection","features":[]}"#,
            r#"{"name":"x","features":[]}"#,
            r#"{"features":[{"id":1,"geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]}}]}"#,
            r#"{"features":[{"geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]}}]}"#,
            r#"{"features":[{"id":"a","properties":{"n":1},"geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]}}]}"#,
            r#"{"features":[{"id":"a","bbox":[0,0,1,1],"geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]}}]}"#,
            r#"{"features":[{"id":"a","geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1]]]}}]}"#,
            r#"{"features":[{"id":"a","geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[0,0]]]}}]}"#,
            r#"{"features":[{"id":"a","geometry":{"type":"Polygon","coordinates":[]}}]}"#,
            r#"{"features":[{"id":"a","geometry":null}]}"#,
            r#"{"features":[{"id":"a"}]}"#,
            r#"{"features":[{"id":"a","geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]],}}]}"#,
            r#"{"features":[{"id":"a","geometry":{"type":"Polygon","coordinates":[[[0,0],[01,0],[1,1],[0,0]]]}}]}"#,
            r#"{"features":[{"id":"a","geometry":{"type":"Polygon","crs":"x"}}]}"#,
        ] {
            assert!(matches!(read(bad), Err(LocationsError::Fails(_))), "{bad}");
        }
        assert!(matches!(
            read_locations(b"\xff\xfe{\x00"),
            Err(LocationsError::Unread(_))
        ));
    }

    #[test]
    fn json_strings_and_numbers() {
        let mut lx = Lexer::new(br#"["a\u00e9\n\ud83d\ude00", -0.5e-3, 12, true, null]"#);
        let toks: Vec<Tok> = std::iter::from_fn(|| lx.next().unwrap()).collect();
        assert_eq!(
            toks,
            vec![
                Tok::StartArray,
                Tok::Str("a\u{e9}\n\u{1f600}".into()),
                Tok::Num(-0.0005),
                Tok::Num(12.0),
                Tok::Literal,
                Tok::Literal,
                Tok::EndArray
            ]
        );
        for bad in [
            &br#"[1.]"#[..],
            br#"[.5]"#,
            br#"[+1]"#,
            br#"["\x"]"#,
            b"[\"a\x01\"]",
            br#"[nul]"#,
        ] {
            let mut lx = Lexer::new(bad);
            let r: Result<Vec<_>, _> = std::iter::from_fn(|| lx.next().transpose()).collect();
            assert!(r.is_err(), "{}", String::from_utf8_lossy(bad));
        }
    }
}
