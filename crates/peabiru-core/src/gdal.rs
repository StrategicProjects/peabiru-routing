//! How GDAL's GeoJSON driver writes a coordinate, as `sf::st_write()` calls
//! it (no layer options: no `COORDINATE_PRECISION`, no
//! `SIGNIFICANT_FIGURES`, not RFC 7946). The speed polygons of a scenario
//! reach R5 through such a file, so R5 routes on the coordinates read back
//! from that text; the bindings without GDAL (Python, JavaScript) apply the
//! same rule.
//!
//! The rule, from GDAL 3.8.5 (`OGR_json_double_with_precision_to_string`
//! in `ogrgeojsonwriter.cpp`, `OGRFormatDouble`, `intelliround` and
//! `removeTrailingZeros` in `ogrutils.cpp`; the same in GDAL 3.6.0 and
//! 3.10.0): a magnitude above 1e50 is written with `%.18g` (which reads
//! back exactly); any other finite value in fixed notation with 15
//! decimals, then "intelligently rounded" (a tail of five zeros or five
//! nines before the last digit, or of seven to two in larger numbers, is
//! taken for binary round-off: dropped, or rounded up), then without
//! trailing zeros (keeping one after the point).

/// The text GDAL writes for `v` (checked against sf::st_write on 3.5
/// million doubles); above 1e50 in magnitude GDAL writes `%.18g` and this
/// the shortest exact form, which reads back as the same double.
pub fn gdal_geojson_text(v: f64) -> String {
    if v.is_nan() {
        return "nan".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf" } else { "-inf" }.into();
    }
    if v.abs() > 1e50 {
        // %.18g: 17 significant digits read back exactly, as does Rust's
        // shortest form (the text differs, the value does not)
        return format!("{v:e}");
    }
    remove_trailing_zeros(intelliround(format!("{v:.15}")))
}

/// The coordinate R5 reads for `v` in such a file: GDAL's text for it
/// ([`gdal_geojson_text`]) read back as the nearest double, as Java's
/// `Double.parseDouble` and GDAL read it. Infinite and NaN values are
/// returned as they are.
pub fn gdal_geojson_coordinate(v: f64) -> f64 {
    if !v.is_finite() || v.abs() > 1e50 {
        return v;
    }
    gdal_geojson_text(v)
        .parse()
        .expect("GDAL's fixed notation is a decimal number")
}

/// `intelliround` of GDAL 3.8.5, on the text of `std::fixed` output.
fn intelliround(mut s: String) -> String {
    let Some(dot) = s.find('.') else {
        return s;
    };
    if s.contains(['e', 'E']) {
        return s;
    }
    let b = s.as_bytes();
    let mut before = dot - 1;
    if b[0] == b'-' {
        before -= 1;
    }
    let i = b.len();
    if i <= 10 {
        return s;
    }
    let all = |c: u8, from: usize, to: usize| (from..=to).all(|k| b[i - k] == c);
    // the digits at i-3 .. i-7 must be `c` unless the number has at least
    // 4 .. 8 digits before the point
    let tail =
        |c: u8| (3..=7).all(|k| before > k || b[i - k] == c) && b[i - 8] == c && b[i - 9] == c;
    if all(b'0', 2, 6) {
        s.pop();
    } else if dot < i - 8 && tail(b'0') {
        s.truncate(i - 8);
    } else if all(b'9', 2, 6) {
        s.truncate(i - 6);
        s = roundup(s);
    } else if dot < i - 9 && tail(b'9') {
        s.truncate(i - 9);
        s = roundup(s);
    }
    s
}

/// GDAL's `roundup`: add 1 to the last digit, carrying.
fn roundup(s: String) -> String {
    let (negative, digits) = match s.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, s.as_str()),
    };
    let mut d: Vec<u8> = digits.bytes().collect();
    let mut pos = d.len();
    while pos > 0 {
        pos -= 1;
        if d[pos] == b'.' {
            continue;
        }
        d[pos] += 1;
        if d[pos] != b':' {
            break;
        }
        d[pos] = b'0';
        if pos == 0 {
            d.insert(0, b'1');
        }
    }
    let mut out = String::with_capacity(d.len() + 1);
    if negative {
        out.push('-');
    }
    out.push_str(std::str::from_utf8(&d).expect("ASCII"));
    out
}

/// GDAL's `removeTrailingZeros`: no zeros after the last nonzero decimal,
/// but one digit after the point.
fn remove_trailing_zeros(mut s: String) -> String {
    if !s.contains('.') {
        return s;
    }
    let keep = s.trim_end_matches('0').len();
    s.truncate(keep);
    if s.ends_with('.') {
        s.push('0');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Texts GDAL 3.8.5 writes (checked with sf::st_write).
    #[test]
    fn as_gdal() {
        for (v, t) in [
            (-30.000000000000004, "-30.0"),
            (-51.2, "-51.2"),
            (0.1 + 0.2, "0.3"),
            (-0.0, "-0.0"),
            (0.0, "0.0"),
            (1e-20, "0.0"),
            (-1e-20, "-0.0"),
            (12345.000000011, "12345.000000010999429"),
            (12345678.9999999, "12345678.999999899417162"),
            (45.12300000001, "45.123000000010002"),
            (1.0000000000001, "1.0000000000001"),
            (1.0 / 3.0, "0.333333333333333"),
            (2.0 / 3.0, "0.666666666666667"),
            (123.4567, "123.4567"),
            (0.9999999999999, "1.0"),
            (0.99999999999999, "1.0"),
        ] {
            assert_eq!(gdal_geojson_text(v), t, "{v:e}");
        }
        assert_eq!(gdal_geojson_coordinate(-30.000000000000004), -30.0);
        assert_eq!(gdal_geojson_coordinate(1e60), 1e60);
    }
}
