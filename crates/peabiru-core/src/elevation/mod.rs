//! Slope costs for walking and cycling from an elevation raster, as R5 7.6
//! adds them for r5r's `build_network(elevation = "TOBLER" | "MINETTI")`
//! (`RasterCost`, `ElevationLoader`, `RasterDataSourceSampler`,
//! `ElevationCostField`, `ToblerCalculator`, `MinettiCalculator`,
//! `MultistageTraversalTimeCalculator`).
//!
//! When the network is built (after the transit stops and park-and-rides
//! are linked, so every edge is covered):
//!
//! - every vertex is sampled at its coordinates, and every edge pair of at
//!   least 10 m every 10 m along its geometry (a flat projection scaled by
//!   the cosine of the first point's latitude, 111,111.111 m a degree), at
//!   the nearest pixel, as GeoTools evaluates a coverage without
//!   interpolation; points outside the raster get 0 m;
//! - each sample is kept in whole decimetres (`Math.round`, then a 16-bit
//!   integer);
//! - each directed edge gets a factor: the slope cost function averaged
//!   over the segments between the samples, weighted by their ground
//!   length, minus 1, as a 32-bit float.
//!
//! When walking or cycling (never driving) along a whole edge, the
//! traversal time `t` becomes `t + round(t * factor)` (a float product,
//! `Math.round(float)`), at least 1 s. Partial edges at the origin and the
//! destination keep their plain times, as in R5.

pub(crate) mod tiff;

use crate::fastmath;
use crate::fastmath_trig;
use crate::street::StreetNetwork;
use tiff::Raster;

/// The slope cost function of a network (r5r's `elevation` argument).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Elevation {
    /// No slope costs (`"NONE"`).
    #[default]
    None,
    /// Tobler's hiking function (`"TOBLER"`, r5r's default when the data
    /// directory has a raster).
    Tobler,
    /// Minetti's energy cost of walking (`"MINETTI"`).
    Minetti,
}

impl Elevation {
    /// The name r5r gives it: `"NONE"`, `"TOBLER"` or `"MINETTI"`.
    pub fn r5r_name(self) -> &'static str {
        match self {
            Elevation::None => "NONE",
            Elevation::Tobler => "TOBLER",
            Elevation::Minetti => "MINETTI",
        }
    }

    /// The model of an r5r name, in any case (r5r upper-cases it); `None`
    /// for any other text.
    pub fn from_r5r_name(name: &str) -> Option<Self> {
        match name.to_ascii_uppercase().as_str() {
            "NONE" => Some(Elevation::None),
            "TOBLER" => Some(Elevation::Tobler),
            "MINETTI" => Some(Elevation::Minetti),
            _ => None,
        }
    }
}

/// `ElevationLoader.ELEVATION_SAMPLE_SPACING_METERS`.
const SPACING_M: f64 = 10.0;
/// `com.conveyal.gtfs.util.Util.METERS_PER_DEGREE_LATITUDE`.
const METERS_PER_DEGREE: f64 = 111_111.111;
/// `VertexStore.FIXED_FACTOR`.
const FIXED: f64 = 1e7;

/// Java's `Math.round(double)`: the closest `long`, ties toward positive
/// infinity, 0 for NaN, saturating.
pub(crate) fn java_round(x: f64) -> i64 {
    if x.is_nan() {
        return 0;
    }
    let f = x.floor();
    // exact: |x| < 2^52 leaves room for the fraction, and beyond x is whole
    (if x - f >= 0.5 { f + 1.0 } else { f }) as i64
}

/// Java's `Math.round(float)`: the closest `int`, ties toward positive
/// infinity, 0 for NaN, saturating.
pub(crate) fn java_round_f32(x: f32) -> i32 {
    if x.is_nan() {
        return 0;
    }
    let f = x.floor();
    (if x - f >= 0.5 { f + 1.0 } else { f }) as i32
}

/// A whole-edge walking or cycling time with the edge's slope factor
/// (`MultistageTraversalTimeCalculator` with one `ElevationCostField`).
#[inline]
pub(crate) fn sloped_seconds(base: i32, factor: f32) -> i32 {
    let add = java_round_f32(base as f32 * factor);
    base.wrapping_add(add).max(1)
}

/// Why the elevation could not be applied, without the raster's name.
pub(crate) type ElevResult<T> = Result<T, String>;

/// Elevations sampled on a network, in decimetres: one per vertex, and
/// per edge pair the points every 10 m inside it (forward order).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Samples {
    pub(crate) vertex_dm: Vec<i16>,
    pub(crate) profile_dm: Vec<Vec<i16>>,
}

/// `RasterDataSourceSampler.readElevation`: the raster value at the pixel
/// nearest `(lon, lat)`, 0 outside the raster.
fn read_elevation(r: &Raster<'_>, lon: f64, lat: f64) -> ElevResult<f64> {
    match r.georef.pixel(lon, lat, r.width, r.height) {
        Ok(None) => Ok(0.0),
        Ok(Some((x, y))) => r.value(x, y),
        Err(()) => Err(format!(
            "the point ({lon}, {lat}) lies on the outer edge of the raster, where GeoTools \
             finds no pixel and r5r fails to build the network; extend the raster by a pixel"
        )),
    }
}

/// `ElevationLoader.clampToShort(Math.round(m * 10))`, an error where R5
/// would wrap the value around (it logs "int value overflowed short").
fn to_dm(m: f64, lon: f64, lat: f64) -> ElevResult<i16> {
    let d = java_round(m * 10.0);
    i16::try_from(d).map_err(|_| {
        format!(
            "the raster gives an elevation of {m} m at ({lon}, {lat}), beyond the +/-3276.7 m \
             that R5 keeps (in decimetres, as 16-bit integers): r5r would store a wrong \
             elevation there; check the raster's no-data value and units"
        )
    })
}

/// `RasterDataSourceSampler.EdgeSampler` over the points of pair `k`.
fn sample_pair(net: &StreetNetwork, r: &Raster<'_>, k: usize) -> ElevResult<Vec<i16>> {
    if f64::from(net.pair_length_mm[k]) < SPACING_M * 1000.0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut first = true;
    let (mut cos_lat, mut prev_lon, mut prev_lat) = (0.0, 0.0, 0.0);
    let mut to_next = SPACING_M;
    for (flat, flon) in net.pair_points(k) {
        let lon = f64::from(flon) / FIXED;
        let lat = f64::from(flat) / FIXED;
        if first {
            cos_lat = fastmath_trig::cos(fastmath_trig::to_radians(lat));
            first = false;
        } else {
            let dx = lon - prev_lon;
            let dy = lat - prev_lat;
            let dx_eq = cos_lat * dx;
            let length = (dx_eq * dx_eq + dy * dy).sqrt() * METERS_PER_DEGREE;
            let (mut cur_lon, mut cur_lat) = (prev_lon, prev_lat);
            let mut remaining = length;
            while remaining >= to_next {
                let frac = to_next / length;
                cur_lon += dx * frac;
                cur_lat += dy * frac;
                let m = read_elevation(r, cur_lon, cur_lat)?;
                out.push(to_dm(m, cur_lon, cur_lat)?);
                remaining -= to_next;
                to_next = SPACING_M;
            }
            to_next -= remaining;
        }
        prev_lon = lon;
        prev_lat = lat;
    }
    Ok(out)
}

/// Sample the raster on every vertex and edge pair of `net`
/// (`ElevationLoader.load`), with up to `threads` workers.
pub(crate) fn sample(net: &StreetNetwork, r: &Raster<'_>, threads: usize) -> ElevResult<Samples> {
    let nv = net.n_vertices();
    let np = net.pair_length_mm.len();
    let threads = threads.clamp(1, 64);
    let vchunk = nv.div_ceil(threads).max(1);
    let vparts = crate::sys::scoped(0..threads, |w| -> ElevResult<Vec<i16>> {
        let lo = (w * vchunk).min(nv);
        let hi = ((w + 1) * vchunk).min(nv);
        (lo..hi)
            .map(|v| {
                let lon = f64::from(net.v_lon[v]) / FIXED;
                let lat = f64::from(net.v_lat[v]) / FIXED;
                to_dm(read_elevation(r, lon, lat)?, lon, lat)
            })
            .collect()
    });
    let mut vertex_dm = Vec::with_capacity(nv);
    for p in vparts {
        vertex_dm.extend(p?);
    }
    let pchunk = np.div_ceil(threads).max(1);
    let pparts = crate::sys::scoped(0..threads, |w| -> ElevResult<Vec<Vec<i16>>> {
        let lo = (w * pchunk).min(np);
        let hi = ((w + 1) * pchunk).min(np);
        (lo..hi).map(|k| sample_pair(net, r, k)).collect()
    });
    let mut profile_dm = Vec::with_capacity(np);
    for p in pparts {
        profile_dm.extend(p?);
    }
    Ok(Samples {
        vertex_dm,
        profile_dm,
    })
}

/// `ToblerCalculator.tobler`.
fn tobler(dx: f64, dy: f64) -> f64 {
    1.2 * fastmath::exp(-3.5 * (dy / dx + 0.05).abs())
}

/// `MinettiCalculator.minettiCwi`: the polynomial of Minetti et al.
/// (2002) for walking, by Horner's rule as commons-math3's
/// `PolynomialFunction.evaluate`, normalised to 1 on flat ground.
fn minetti(dx: f64, dy: f64) -> f64 {
    const C: [f64; 6] = [2.5, 19.6, 51.9, -76.8, -58.7, 280.5];
    let horner = |x: f64| {
        let mut r = C[5];
        for j in (0..5).rev() {
            r = x * r + C[j];
        }
        r
    };
    // `if (!(Math.abs(gradient) < 0.5))` clamps to [-0.5, 0.5]; NaN stays
    let g = (dy / dx).clamp(-0.5, 0.5);
    horner(g) * (1.0 / horner(0.0))
}

/// `ElevationCostField.computeWeightedAverages`: per directed edge, the
/// slope factor (cost relative to flat ground, minus 1).
pub(crate) fn edge_factors(net: &StreetNetwork, s: &Samples, model: Elevation) -> Vec<f32> {
    let f: fn(f64, f64) -> f64 = match model {
        Elevation::Tobler => tobler,
        Elevation::Minetti => minetti,
        Elevation::None => return Vec::new(),
    };
    let elev = |dm: i16| f64::from(dm) / 10.0;
    (0..net.n_edges())
        .map(|e| {
            let k = e / 2;
            let invert = e % 2 == 1;
            let delta = |prev: f64, cur: f64| if invert { prev - cur } else { cur - prev };
            let (mut sum, mut dist) = (0.0f64, 0.0f64);
            let mut consume = |dx: f64, dy: f64| {
                sum += dx * f(dx, dy);
                dist += dx;
            };
            let mut remaining = f64::from(net.pair_length_mm[k]) / 1000.0;
            let mut prev = elev(s.vertex_dm[net.pair_from[k] as usize]);
            for &d in &s.profile_dm[k] {
                let cur = elev(d);
                consume(SPACING_M, delta(prev, cur));
                remaining -= SPACING_M;
                prev = cur;
            }
            if (0.0..=SPACING_M + 0.5).contains(&remaining) {
                consume(
                    remaining,
                    delta(prev, elev(s.vertex_dm[net.pair_to[k] as usize])),
                );
            }
            let weighted = match model {
                Elevation::Tobler => 1.0 / (sum / dist),
                _ => sum / dist,
            };
            ((weighted - 1.0) * 1.0) as f32
        })
        .collect()
}

/// Sample `raster` on `net` and give it the slope factors of `model`.
pub(crate) fn apply(
    net: &mut StreetNetwork,
    raster: &[u8],
    model: Elevation,
    threads: usize,
) -> ElevResult<()> {
    if model == Elevation::None {
        return Ok(());
    }
    let r = Raster::parse(raster)?;
    let s = sample(net, &r, threads)?;
    net.edge_slope = edge_factors(net, &s, model);
    net.elevation = model;
    net.forget_kept();
    Ok(())
}

/// [`read_elevation`] at each point of `points`, for comparing with R5's.
pub(crate) fn read_points(
    raster: &[u8],
    points: &[(f64, f64)],
) -> ElevResult<Vec<ElevResult<f64>>> {
    let r = Raster::parse(raster)?;
    Ok(points
        .iter()
        .map(|&(lon, lat)| read_elevation(&r, lon, lat))
        .collect())
}

/// The samples of `raster` on `net`, for comparing with R5's.
pub(crate) fn samples_of(net: &StreetNetwork, raster: &[u8]) -> ElevResult<Samples> {
    let r = Raster::parse(raster)?;
    sample(net, &r, crate::sys::cores())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_rounding() {
        assert_eq!(java_round(2.5), 3);
        assert_eq!(java_round(-2.5), -2);
        assert_eq!(java_round(-2.500001), -3);
        assert_eq!(java_round(0.49999999999999994), 0);
        assert_eq!(java_round(f64::NAN), 0);
        assert_eq!(java_round(1e300), i64::MAX);
        assert_eq!(java_round_f32(-0.5), 0);
        assert_eq!(java_round_f32(0.5), 1);
        assert_eq!(java_round_f32(f32::NAN), 0);
        assert_eq!(java_round_f32(-1.5), -1);
    }

    #[test]
    fn sloped_times() {
        assert_eq!(sloped_seconds(10, 0.0), 10);
        assert_eq!(sloped_seconds(10, 0.25), 13); // 2.5 rounds up
        assert_eq!(sloped_seconds(10, -0.25), 8); // -2.5 rounds up
        assert_eq!(sloped_seconds(1, -0.9), 1);
        assert_eq!(sloped_seconds(0, 0.3), 1);
        assert_eq!(sloped_seconds(7, f32::NAN), 7);
    }

    #[test]
    fn cost_functions_on_flat_ground() {
        // Tobler peaks on a slight downhill: flat ground is a bit slower
        assert_eq!(tobler(10.0, 0.0), 1.2 * fastmath::exp(-3.5 * 0.05));
        assert!(tobler(10.0, -0.5) > tobler(10.0, 0.0));
        assert_eq!(minetti(10.0, 0.0), 1.0);
        // clamped beyond a gradient of 0.5
        assert_eq!(minetti(10.0, 9.0), minetti(10.0, 5.0));
    }

    #[test]
    fn names() {
        assert_eq!(Elevation::from_r5r_name("tobler"), Some(Elevation::Tobler));
        assert_eq!(
            Elevation::from_r5r_name("Minetti"),
            Some(Elevation::Minetti)
        );
        assert_eq!(Elevation::from_r5r_name("none"), Some(Elevation::None));
        assert_eq!(Elevation::from_r5r_name("hills"), None);
        assert_eq!(Elevation::Tobler.r5r_name(), "TOBLER");
    }
}
