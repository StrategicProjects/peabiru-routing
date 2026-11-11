//! Geometry and number formatting of r5r's itineraries: edge lines as R5's
//! `Edge.getGeometry()` gives them, `Utils.getLinestringLength`, the
//! `TripLeg.getGeometry()` clean-up, JTS's WKT round trip and r5r's
//! rounding of minutes.

use crate::street::StreetNetwork;

/// A line as `(lon, lat)` degrees.
pub(crate) type Line = Vec<[f64; 2]>;

/// `Edge.getGeometry()` of directed edge `e`: the pair's points in floating
/// degrees (`fixed / 1e7`), reversed for a backward edge.
pub(crate) fn edge_line(net: &StreetNetwork, e: usize) -> Line {
    let mut pts: Line = net
        .pair_points(e / 2)
        .map(|(lat, lon)| [f64::from(lon) / 1e7, f64::from(lat) / 1e7])
        .collect();
    if e % 2 == 1 {
        pts.reverse();
    }
    pts
}

/// `Utils.getLinestringLength`: the haversine metres of every segment
/// (R5's `GeometryUtils.distance`), summed and rounded (`Math.round`).
pub(crate) fn line_length_m(line: &[[f64; 2]]) -> i32 {
    let mut acc = 0.0f64;
    for w in line.windows(2) {
        acc += crate::fastmath_trig::haversine_m(w[0][1], w[0][0], w[1][1], w[1][0]);
    }
    java_round(acc) as i32
}

/// `Math.round(double)` for the values met here (finite, not huge).
pub(crate) fn java_round(x: f64) -> i64 {
    (x + 0.5).floor() as i64
}

/// `Utils.roundTo1Place(seconds / 60.0)`: minutes to one decimal place.
pub(crate) fn minutes_1dp(seconds: i32) -> f64 {
    java_round(f64::from(seconds) / 60.0 * 10.0) as f64 / 10.0
}

/// `Utils.getTimeFromSeconds`: `HH:MM:SS`, hours past 24 as they come.
pub(crate) fn clock(seconds: i32) -> String {
    let s = seconds % 60;
    let m = (seconds / 60) % 60;
    let h = (seconds / 60) / 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// `TripLeg.getGeometry()`: consecutive repeated points dropped, unless
/// fewer than two would be left.
pub(crate) fn without_repeats(line: &[[f64; 2]]) -> Line {
    let mut out: Line = Vec::with_capacity(line.len());
    for &p in line {
        if out.last() != Some(&p) {
            out.push(p);
        }
    }
    if out.len() >= 2 && out.len() < line.len() {
        out
    } else {
        line.to_vec()
    }
}

/// The double sf reads back from the WKT JTS writes for `x`: JTS writes at
/// most 16 digits after the decimal point (`OrdinateFormat`, a
/// `DecimalFormat` rounding half-even), so only values whose shortest
/// representation has more fraction digits (below 1 in absolute value,
/// roughly) change.
pub(crate) fn wkt_roundtrip(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let s = format!("{}", x.abs());
    let Some(dot) = s.find('.') else { return x };
    let frac = &s[dot + 1..];
    if frac.len() <= 16 {
        return x;
    }
    let mut digits: Vec<u8> = s[..dot]
        .bytes()
        .chain(frac[..16].bytes())
        .map(|b| b - b'0')
        .collect();
    let rest = &frac[16..];
    let first = rest.as_bytes()[0] - b'0';
    let tail_zero = rest[1..].bytes().all(|b| b == b'0');
    let last_odd = digits.last().is_some_and(|d| d % 2 == 1);
    let up = first > 5 || (first == 5 && (!tail_zero || last_odd));
    if up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, 1);
                break;
            }
            i -= 1;
            if digits[i] == 9 {
                digits[i] = 0;
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let n_int = digits.len() - 16;
    let mut t = String::with_capacity(digits.len() + 2);
    for (i, d) in digits.iter().enumerate() {
        if i == n_int {
            t.push('.');
        }
        t.push((b'0' + d) as char);
    }
    let v: f64 = t.parse().expect("a decimal number");
    if x < 0.0 {
        -v
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wkt_roundtrip_matches_jts() {
        // values printed by JTS 1.20 (LineString.toString) and read by sf
        assert_eq!(wkt_roundtrip(-51.123456789012345), -51.123456789012345);
        assert_eq!(wkt_roundtrip(0.1 + 0.2), 0.3);
        assert_eq!(wkt_roundtrip(1.0 / 3.0), 1.0 / 3.0);
        assert_eq!(wkt_roundtrip(-0.000012345678901234567), -0.0000123456789012);
        assert_eq!(wkt_roundtrip(1e-20), 0.0);
        assert_eq!(wkt_roundtrip(5e-8), 5e-8);
        assert_eq!(wkt_roundtrip(3e-7), 3e-7);
    }

    #[test]
    fn minutes_round_like_java() {
        assert_eq!(minutes_1dp(0), 0.0);
        assert_eq!(minutes_1dp(3), 0.1); // 0.05 min -> 0.5 -> 1
        assert_eq!(minutes_1dp(1234), 20.6);
        assert_eq!(clock(14 * 3600 + 5 * 60 + 7), "14:05:07");
        assert_eq!(clock(25 * 3600), "25:00:00");
    }
}
