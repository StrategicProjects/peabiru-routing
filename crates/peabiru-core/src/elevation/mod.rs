//! Slope costs for walking and cycling from an elevation raster, as R5 7.6
//! adds them with Tobler's or Minetti's cost function
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

pub(crate) mod crs;
pub(crate) mod tiff;

use crate::fastmath;
use crate::fastmath_trig;
use crate::street::StreetNetwork;
use tiff::{Near, Raster};

/// The slope cost function of a network.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Elevation {
    /// No slope costs (`"NONE"`).
    #[default]
    None,
    /// Tobler's hiking function (`"TOBLER"`).
    Tobler,
    /// Minetti's energy cost of walking (`"MINETTI"`).
    Minetti,
}

impl Elevation {
    /// Its name: `"NONE"`, `"TOBLER"` or `"MINETTI"`.
    pub fn name(self) -> &'static str {
        match self {
            Elevation::None => "NONE",
            Elevation::Tobler => "TOBLER",
            Elevation::Minetti => "MINETTI",
        }
    }

    /// The model of a name, in upper or lower case; `None`
    /// for any other text.
    pub fn from_name(name: &str) -> Option<Self> {
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
/// nearest `(lon, lat)` once in the raster's coordinate reference system,
/// 0 outside the raster. A sample where R5's own result depends on the
/// machine (see below) is read as Java's `StrictMath` reads it and counted
/// in `s.ambiguous`.
fn read_elevation(r: &Raster<'_>, lon: f64, lat: f64, s: &mut Suspect) -> ElevResult<f64> {
    let (x, y) = r.crs.apply(lon, lat);
    let m = match read_at(r, x, y) {
        Ok(m) => m,
        // StrictMath's own point is inside the envelope but past the last
        // pixel: GeoTools throws there and R5 fails, even on a JVM whose
        // sines are StrictMath's; there is no pixel to read
        Err(None) => {
            return Err(format!(
                "the point ({lon}, {lat}) lies on the outer edge of the raster, where there is \
                 no pixel to read; extend the raster by a pixel"
            ))
        }
        Err(Some(e)) => return Err(e),
    };
    if let Some(margin) = r.crs.margin() {
        // Where the JVM's sine or cosine could put the point in another
        // pixel than StrictMath's (see `crs`), R5's own result depends on
        // the machine. Within `margin` of the point, the pixels it may
        // fall in are those at the corners of a square of side `2 margin`
        // (much smaller than a pixel): if they all give the elevation R5
        // stores (in decimetres), the answer is the same on every machine.
        // Near the envelope's edge R5 elsewhere may also read 0 m outside
        // or fail on the sliver past the last pixel: always ambiguous.
        // peabiru keeps StrictMath's pixel, the same on every machine.
        let near = r.georef.near_boundary(x, y, margin);
        if near != Near::No {
            let dm = |m: f64| java_round(m * 10.0) as i16;
            let corners = [(-1.0, -1.0), (-1.0, 1.0), (1.0, -1.0), (1.0, 1.0)];
            let same = near == Near::Pixel
                && corners.iter().all(|&(sx, sy)| {
                    read_at(r, x + sx * margin, y + sy * margin).is_ok_and(|c| dm(c) == dm(m))
                });
            if !same {
                s.ambiguous += 1;
            }
        }
    }
    Ok(m)
}

/// The value R5 reads at `(x, y)` in the raster's system: 0 outside its
/// envelope, `Err(None)` on its outer edge (inside the envelope, past the
/// last pixel, where GeoTools throws), `Err(Some(_))` for a raster that
/// cannot be decoded there.
fn read_at(r: &Raster<'_>, x: f64, y: f64) -> Result<f64, Option<String>> {
    match r.georef.pixel(x, y, r.width, r.height) {
        Ok(None) => Ok(0.0),
        Ok(Some((px, py))) => r.value(px, py).map_err(Some),
        Err(()) => Err(None),
    }
}

/// Samples that are almost certainly not elevations, for the warning.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Suspect {
    /// Beyond what 16-bit decimetres hold, stored wrapped around.
    pub(crate) wrapped: usize,
    /// Equal to the raster's declared no-data value.
    pub(crate) nodata: usize,
    /// Within nanometres of a boundary between pixels of different values
    /// (or of the raster's edge), where R5 reads one or the other
    /// depending on the machine it runs on.
    pub(crate) ambiguous: usize,
}

impl Suspect {
    fn add(&mut self, o: Suspect) {
        self.wrapped += o.wrapped;
        self.nodata += o.nodata;
        self.ambiguous += o.ambiguous;
    }
}

/// `ElevationLoader.clampToShort(Math.round(m * 10))`: decimetres as a
/// 16-bit integer, wrapped around beyond +/-3276.7 m as Java's `(short)`
/// cast does (R5 logs "int value overflowed short" and keeps the wrapped
/// value: SRTM's -32768 voids become 0 m, -9999 becomes 3108.2 m).
fn to_dm(m: f64, nodata: Option<f64>, s: &mut Suspect) -> i16 {
    let d = java_round(m * 10.0);
    let dm = d as i16;
    if i64::from(dm) != d {
        s.wrapped += 1;
    } else if nodata.is_some_and(|v| v == m || (v.is_nan() && m.is_nan())) {
        s.nodata += 1;
    }
    dm
}

/// `RasterDataSourceSampler.EdgeSampler` over the points of pair `k`.
fn sample_pair(
    net: &StreetNetwork,
    r: &Raster<'_>,
    k: usize,
    s: &mut Suspect,
) -> ElevResult<Vec<i16>> {
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
                let m = read_elevation(r, cur_lon, cur_lat, s)?;
                out.push(to_dm(m, r.nodata, s));
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
pub(crate) fn sample(
    net: &StreetNetwork,
    r: &Raster<'_>,
    threads: usize,
) -> ElevResult<(Samples, Suspect)> {
    let nv = net.n_vertices();
    let np = net.pair_length_mm.len();
    let threads = threads.clamp(1, 64);
    let vchunk = nv.div_ceil(threads).max(1);
    let vparts = crate::sys::scoped(0..threads, |w| -> ElevResult<(Vec<i16>, Suspect)> {
        let lo = (w * vchunk).min(nv);
        let hi = ((w + 1) * vchunk).min(nv);
        let mut s = Suspect::default();
        let v = (lo..hi)
            .map(|v| {
                let lon = f64::from(net.v_lon[v]) / FIXED;
                let lat = f64::from(net.v_lat[v]) / FIXED;
                let m = read_elevation(r, lon, lat, &mut s)?;
                Ok(to_dm(m, r.nodata, &mut s))
            })
            .collect::<ElevResult<Vec<i16>>>()?;
        Ok((v, s))
    });
    let mut suspect = Suspect::default();
    let mut vertex_dm = Vec::with_capacity(nv);
    for p in vparts {
        let (v, s) = p?;
        vertex_dm.extend(v);
        suspect.add(s);
    }
    let pchunk = np.div_ceil(threads).max(1);
    let pparts = crate::sys::scoped(0..threads, |w| -> ElevResult<(Vec<Vec<i16>>, Suspect)> {
        let lo = (w * pchunk).min(np);
        let hi = ((w + 1) * pchunk).min(np);
        let mut s = Suspect::default();
        let v = (lo..hi)
            .map(|k| sample_pair(net, r, k, &mut s))
            .collect::<ElevResult<Vec<Vec<i16>>>>()?;
        Ok((v, s))
    });
    let mut profile_dm = Vec::with_capacity(np);
    for p in pparts {
        let (v, s) = p?;
        profile_dm.extend(v);
        suspect.add(s);
    }
    Ok((
        Samples {
            vertex_dm,
            profile_dm,
        },
        suspect,
    ))
}

/// The warnings for the suspect samples of a raster: one about the
/// values (wrapped, no-data), one about the samples whose pixel depends
/// on the machine; none without any.
fn suspect_warnings(s: Suspect, n: usize, nodata: Option<f64>, crs: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut parts = Vec::new();
    if s.wrapped > 0 {
        parts.push(format!(
            "{} with a value beyond +/-3276.7 m, which is stored wrapped around as 16-bit \
             decimetres (an SRTM void of -32768 becomes 0 m, -9999 becomes 3108.2 m, 3500 m \
             becomes -3053.6 m)",
            s.wrapped
        ));
    }
    if s.nodata > 0 {
        parts.push(format!(
            "{} equal to the raster's no-data value ({}), which is {}",
            s.nodata,
            nodata.map_or_else(String::new, |v| v.to_string()),
            if nodata.is_some_and(f64::is_nan) {
                "stored as 0 m"
            } else {
                "read as an elevation"
            }
        ));
    }
    if !parts.is_empty() {
        out.push(format!(
            "of the {n} elevation samples on the network, {}; the slope costs follow from \
             these values, and are probably not what you want there: fill the voids or crop the raster",
            parts.join(", and ")
        ));
    }
    if s.ambiguous > 0 {
        out.push(format!(
            "{} of the {n} elevation samples on the network lie within nanometres of a \
             boundary between pixels of different values (or of the raster's edge) once in \
             {crs}: which pixel R5 reads there depends on the machine it runs on \
             (the last bits of Java's sines and cosines); peabiru reads the one Java's \
             StrictMath gives, on every machine. Reprojecting the raster to EPSG:4326 \
             (gdalwarp -t_srs EPSG:4326) removes the ambiguity",
            s.ambiguous
        ));
    }
    out
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
    let (s, suspect) = sample(net, &r, threads)?;
    let n = s.vertex_dm.len() + s.profile_dm.iter().map(Vec::len).sum::<usize>();
    net.edge_slope = edge_factors(net, &s, model);
    net.elevation = model;
    net.elevation_warnings = suspect_warnings(suspect, n, r.nodata, &r.crs.label());
    net.forget_kept();
    Ok(())
}

/// [`read_elevation`] at each point of `points`, for comparing with R5's:
/// the value and whether the point is ambiguous (its pixel depends on the
/// machine R5 runs on).
pub(crate) fn read_points(
    raster: &[u8],
    points: &[(f64, f64)],
) -> ElevResult<Vec<ElevResult<(f64, bool)>>> {
    let r = Raster::parse(raster)?;
    Ok(points
        .iter()
        .map(|&(lon, lat)| {
            let mut s = Suspect::default();
            read_elevation(&r, lon, lat, &mut s).map(|m| (m, s.ambiguous > 0))
        })
        .collect())
}

/// The samples of `raster` on `net`, for comparing with R5's.
pub(crate) fn samples_of(net: &StreetNetwork, raster: &[u8]) -> ElevResult<Samples> {
    let r = Raster::parse(raster)?;
    sample(net, &r, crate::sys::cores()).map(|(s, _)| s)
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
    fn decimetres_wrap_around_as_java_short_casts() {
        let mut s = Suspect::default();
        // (short) Math.round(m * 10), as R5's clampToShort
        assert_eq!(to_dm(812.34, None, &mut s), 8123);
        assert_eq!(to_dm(3276.7, None, &mut s), 32767);
        assert_eq!(s, Suspect::default());
        assert_eq!(to_dm(-9999.0, None, &mut s), 31082); // 3108.2 m
        assert_eq!(to_dm(-32768.0, None, &mut s), 0);
        assert_eq!(to_dm(3500.0, None, &mut s), -30536);
        assert_eq!(to_dm(f64::INFINITY, None, &mut s), -1); // Long.MAX_VALUE
        assert_eq!(s.wrapped, 4);
        assert_eq!(to_dm(f64::NAN, None, &mut s), 0);
        assert_eq!(to_dm(0.0, Some(0.0), &mut s), 0);
        assert_eq!(to_dm(f64::NAN, Some(f64::NAN), &mut s), 0);
        assert_eq!(to_dm(5.0, Some(0.0), &mut s), 50);
        assert_eq!(
            s,
            Suspect {
                wrapped: 4,
                nodata: 2,
                ambiguous: 0
            }
        );
        let w = suspect_warnings(s, 100, Some(0.0), "EPSG:4326 (WGS 84)");
        assert_eq!(w.len(), 1);
        assert!(w[0].starts_with("of the 100 elevation samples on the network, 4 with a value"));
        assert!(w[0].contains("and 2 equal to the raster's no-data value (0)"));
        assert!(suspect_warnings(Suspect::default(), 100, None, "").is_empty());
        let a = Suspect {
            ambiguous: 3,
            ..Suspect::default()
        };
        let w = suspect_warnings(a, 100, None, "EPSG:31982 (SIRGAS 2000 / UTM zone 22S)");
        assert_eq!(w.len(), 1);
        assert!(w[0].starts_with("3 of the 100 elevation samples on the network lie within"));
        assert!(
            w[0].contains("once in EPSG:31982 (SIRGAS 2000 / UTM zone 22S): which pixel R5 reads")
        );
        assert!(w[0].contains("StrictMath"));
        // both kinds: two warnings
        let both = Suspect {
            wrapped: 1,
            nodata: 0,
            ambiguous: 1,
        };
        assert_eq!(suspect_warnings(both, 100, None, "x").len(), 2);
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
        assert_eq!(Elevation::from_name("tobler"), Some(Elevation::Tobler));
        assert_eq!(Elevation::from_name("Minetti"), Some(Elevation::Minetti));
        assert_eq!(Elevation::from_name("none"), Some(Elevation::None));
        assert_eq!(Elevation::from_name("hills"), None);
        assert_eq!(Elevation::Tobler.name(), "TOBLER");
    }
}
