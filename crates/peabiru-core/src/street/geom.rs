//! Geometry helpers reproducing R5's arithmetic (Java `double`, `(int)` casts).

/// Radius used for edge lengths (R5 `GeometryUtils.RADIUS_OF_EARTH_M`).
pub const RADIUS_OF_EARTH_M: f64 = 6_367_450.0;
/// Radius and correction used by R5's equirectangular `fastDistance`.
const FAST_RADIUS_M: f64 = 6371.01 * 1000.0;
const MAX_ERR_INV: f64 = 0.999462;
/// Fixed-point factor for degrees.
pub const FIXED_FACTOR: f64 = 1e7;

/// Java `(int) x` for a double: truncation toward zero, saturating, NaN to 0.
#[inline]
pub fn java_int(x: f64) -> i32 {
    x as i32
}

/// R5 node coordinate from OSM 1e-7 degrees. osmlib decodes a PBF coordinate
/// as `nanodegrees * 1e-9` and truncates `lat * 1e7`, which can land one unit
/// below the exact value.
#[inline]
pub fn node_fixed(e7: i32) -> i32 {
    let deg = (i64::from(e7) * 100) as f64 * 1e-9;
    java_int(deg * FIXED_FACTOR)
}

/// R5 vertex coordinate from a node's fixed coordinate: the vertex store
/// converts `fixed / 1e7` back with `(int)(deg * 1e7)`.
#[inline]
pub fn vertex_fixed(node_fixed: i32) -> i32 {
    java_int((f64::from(node_fixed) / FIXED_FACTOR) * FIXED_FACTOR)
}

/// Haversine distance in metres (R5 `GeometryUtils.distance`).
pub fn haversine_m(lat0: f64, lon0: f64, lat1: f64, lon1: f64) -> f64 {
    let t0 = lat0.to_radians();
    let t1 = lat1.to_radians();
    let l0 = lon0.to_radians();
    let l1 = lon1.to_radians();
    let st = ((t1 - t0) / 2.0).sin();
    let sl = ((l1 - l0) / 2.0).sin();
    let under = st * st + t0.cos() * t1.cos() * (sl * sl);
    2.0 * RADIUS_OF_EARTH_M * under.sqrt().asin()
}

/// Equirectangular distance in metres (R5 `SphericalDistanceLibrary.fastDistance`),
/// points as `(lon, lat)` degrees.
fn fast_distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dlat = (b.1 - a.1).to_radians();
    let dlon = (b.0 - a.0).to_radians() * ((a.1 + b.1) / 2.0).to_radians().cos();
    FAST_RADIUS_M * (dlat * dlat + dlon * dlon).sqrt() * MAX_ERR_INV
}

/// Initial bearing from `a` to `b`, radians, points as `(lon, lat)` degrees
/// (R5 `DirectionUtils.getAzimuth`).
fn azimuth(a: (f64, f64), b: (f64, f64)) -> f64 {
    let lat1 = a.1.to_radians();
    let lat2 = b.1.to_radians();
    let dlng = (b.0 - a.0).to_radians();
    let y = dlng.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlng.cos();
    if x.abs() < 1e-10 && y.abs() < 1e-10 {
        return std::f64::consts::PI;
    }
    y.atan2(x)
}

/// Radians to binary radians (`(byte) Math.round(rad * 128 / PI)`).
#[inline]
pub fn radians_to_brads(rad: f64) -> i8 {
    ((rad * 128.0 / std::f64::consts::PI + 0.5).floor() as i64) as i8
}

/// Binary radians to whole degrees (Java int division, toward zero).
#[inline]
pub fn brads_to_degrees(b: i8) -> i32 {
    i32::from(b) * 180 / 128
}

const ANGLE_DIST_M: f64 = 10.0;

/// First and last bearings of a line (R5 `getFirstAngle` / `getLastAngle`),
/// points as `(lon, lat)` degrees.
pub fn first_last_angles(pts: &[(f64, f64)]) -> (f64, f64) {
    let n = pts.len();
    if n < 2 {
        return (0.0, 0.0);
    }
    if n == 2 {
        let a = azimuth(pts[0], pts[1]);
        return (a, a);
    }
    let first = 'f: {
        let mut d = 0.0;
        let mut i = 0;
        while d < ANGLE_DIST_M && i < n - 1 {
            let seg = fast_distance(pts[i], pts[i + 1]);
            if d + seg > ANGLE_DIST_M {
                break 'f azimuth(pts[0], pts[i + 1]);
            }
            d += seg;
            i += 1;
        }
        azimuth(pts[0], pts[n - 1])
    };
    let last = 'l: {
        let mut d = 0.0;
        let mut i = n as isize - 2;
        while d < ANGLE_DIST_M && i >= 0 {
            let iu = i as usize;
            let seg = fast_distance(pts[iu], pts[iu + 1]);
            if d + seg > ANGLE_DIST_M {
                break 'l azimuth(pts[iu], pts[n - 1]);
            }
            d += seg;
            i -= 1;
        }
        azimuth(pts[0], pts[n - 1])
    };
    (first, last)
}

/// Fraction along a segment of the projection of a point, all fixed-point,
/// with R5's stretched projection: longitudes are *divided* by `cos_lat`
/// (`GeometryUtils.segmentFraction` + JTS `LineSegment.segmentFraction`).
pub fn segment_fraction(
    lon0: i32,
    lat0: i32,
    lon1: i32,
    lat1: i32,
    lon: i32,
    lat: i32,
    cos_lat: f64,
) -> f64 {
    let x0 = f64::from(java_int(f64::from(lon0) / cos_lat));
    let x1 = f64::from(java_int(f64::from(lon1) / cos_lat));
    let x = f64::from(java_int(f64::from(lon) / cos_lat));
    let (y0, y1, y) = (f64::from(lat0), f64::from(lat1), f64::from(lat));
    let r = if x == x0 && y == y0 {
        0.0
    } else if x == x1 && y == y1 {
        1.0
    } else {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = dx * dx + dy * dy;
        if len <= 0.0 {
            f64::NAN
        } else {
            ((x - x0) * dx + (y - y0) * dy) / len
        }
    };
    if r < 0.0 {
        0.0
    } else if r > 1.0 || r.is_nan() {
        1.0
    } else {
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brads_roundtrip_like_java() {
        assert_eq!(radians_to_brads(std::f64::consts::PI), -128); // (byte) 128
        assert_eq!(radians_to_brads(-std::f64::consts::PI), -128);
        assert_eq!(radians_to_brads(std::f64::consts::FRAC_PI_2), 64);
        assert_eq!(brads_to_degrees(64), 90);
        assert_eq!(brads_to_degrees(-64), -90);
        assert_eq!(brads_to_degrees(-128), -180);
        assert_eq!(brads_to_degrees(1), 1); // 180/128 truncated
    }

    #[test]
    fn haversine_one_degree_of_latitude() {
        let d = haversine_m(0.0, 0.0, 1.0, 0.0);
        assert!((d - RADIUS_OF_EARTH_M * std::f64::consts::PI / 180.0).abs() < 1e-6);
    }

    #[test]
    fn fixed_point_conversions_truncate() {
        // -30.0538460 deg: 1e-9 * nanodegrees then (int)(x * 1e7)
        let f = node_fixed(-300_538_460);
        assert!((f - -300_538_460).abs() <= 1);
        assert!((vertex_fixed(f) - f).abs() <= 1);
    }

    #[test]
    fn segment_fraction_clamps() {
        let c = 1.0;
        assert_eq!(segment_fraction(0, 0, 10, 0, 5, 3, c), 0.5);
        assert_eq!(segment_fraction(0, 0, 10, 0, -5, 3, c), 0.0);
        assert_eq!(segment_fraction(0, 0, 10, 0, 50, 3, c), 1.0);
        assert_eq!(segment_fraction(0, 0, 0, 0, 5, 3, c), 1.0); // degenerate: NaN -> 1
    }
}
