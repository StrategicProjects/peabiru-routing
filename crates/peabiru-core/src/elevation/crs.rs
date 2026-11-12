//! The coordinate reference systems of elevation rasters other than WGS 84
//! longitude and latitude, and the transform R5 applies to every sampled
//! point (`CRS.findMathTransform(DefaultGeographicCRS.WGS84, crs)` in
//! GeoTools 25.2 with its EPSG database, `gt-epsg-hsql`).
//!
//! The transforms are written from the published formulas, not from
//! GeoTools' code (which is LGPL): the geocentric conversion of EPSG
//! Guidance Note 7-2 (method 9602), its inverse by Toms' non-iterative
//! method (R. Toms, "An improved algorithm for geocentric to geodetic
//! coordinate conversion", 1996, as in NIMA's public-domain `geocent.c`),
//! the geocentric translation (method 9603), the Transverse Mercator series
//! of Snyder (Map Projections: A Working Manual, 1987, equations 8-9 and
//! 8-10, as in PROJ.4's `tmerc`), the American Polyconic (Snyder 18-1 to
//! 18-3, PROJ.4's `poly`) and the spherical Popular Visualisation Pseudo
//! Mercator (method 1024). The order of the floating-point operations
//! (and the way each step derives the eccentricity from the axes) was then
//! matched, by trying the algebraically equal forms, to what GeoTools
//! returns, double for double, on hundreds of thousands of points per
//! system (`bench/compare/crs/crs_parity.sh`, whose Java harness calls
//! GeoTools from r5r's jar). Sines, cosines, tangents, arc tangents and
//! logarithms are fdlibm's ([`crate::fdlibm`]), the values of
//! `java.lang.StrictMath`.
//!
//! **Platforms.** GeoTools calls `java.lang.Math`, whose sine and cosine
//! (and on x86_64 tangent and logarithm) the JVM computes with its own
//! code, which differs from `StrictMath` in the last bit for a few per cent
//! of arguments (measured on arm64: Java 21 gives another sine for 6% of
//! them). r5r's own result then depends on the machine it runs on, by a few
//! nanometres. The sampled pixel is `Math.round` of the grid coordinate, so
//! such a difference only matters when a point falls within a few
//! nanometres of the boundary between two pixels or of the raster's edge.
//! [`Crs::margin`] bounds how far GeoTools' point may be from
//! [`Crs::apply`]'s; the sampler reads the pixel `StrictMath` gives (the
//! same on every machine) and counts the samples that close to a boundary
//! between different values, for a warning.
//!
//! The table of systems ([`lookup`]) is the subset of the EPSG registry
//! that GeoTools' database gives for Brazilian and world-wide data, with
//! the parameters (ellipsoids and datum shifts) exactly as GeoTools has
//! them (read from the jar with `CRS.decode` and `findMathTransform`).

#![allow(clippy::excessive_precision)] // the published constants, as written

use crate::fdlibm;

/// `Math.toRadians` (JDK 9 and later: one multiplication).
#[inline]
fn to_radians(deg: f64) -> f64 {
    deg * 0.017453292519943295
}

/// `Math.toDegrees`.
#[inline]
fn to_degrees(rad: f64) -> f64 {
    rad * 57.29577951308232
}

/// An ellipsoid as GeoTools keeps it: semi-major and semi-minor axes (the
/// minor one derived from the inverse flattening, `a - a / invf`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ellipsoid {
    a: f64,
    b: f64,
}

/// WGS 84 (EPSG:7030).
pub(crate) const WGS84: Ellipsoid = Ellipsoid {
    a: 6378137.0,
    b: 6356752.314245179,
};
/// GRS 1980 (EPSG:7019), of SIRGAS 2000.
pub(crate) const GRS80: Ellipsoid = Ellipsoid {
    a: 6378137.0,
    b: 6356752.314140356,
};
/// GRS 1967 Modified (EPSG:7050), of SAD69.
const GRS67M: Ellipsoid = Ellipsoid {
    a: 6378160.0,
    b: 6356774.719195305,
};
/// International 1924 (EPSG:7022), of Corrego Alegre.
const INTL24: Ellipsoid = Ellipsoid {
    a: 6378388.0,
    b: 6356911.9461279465,
};

/// The geodetic datum of a system, as the step from WGS 84 longitude and
/// latitude to the system's own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Datum {
    /// WGS 84 itself: no step.
    Wgs84,
    /// Another ellipsoid with a zero datum shift (SIRGAS 2000): to
    /// geocentric coordinates on WGS 84 and back on the other ellipsoid.
    Ellipsoid(Ellipsoid),
    /// A geocentric translation (the inverse of the datum's `TOWGS84`)
    /// between the two conversions.
    Shift(Ellipsoid, [f64; 3]),
}

impl Datum {
    fn ellipsoid(&self) -> Ellipsoid {
        match *self {
            Datum::Wgs84 => WGS84,
            Datum::Ellipsoid(e) | Datum::Shift(e, _) => e,
        }
    }
}

const SIRGAS2000: Datum = Datum::Ellipsoid(GRS80);
/// SAD69 (EPSG:6618), `TOWGS84[-59, -11, -52]` in GeoTools' database.
const SAD69: Datum = Datum::Shift(GRS67M, [59.0, 11.0, 52.0]);
/// Corrego Alegre 1970-72 (EPSG:6225), `TOWGS84[-206.05, 168.28, -3.82]`,
/// inverted by GeoTools' matrix inversion (hence the last bits).
const CORREGO_ALEGRE: Datum =
    Datum::Shift(INTL24, [206.05000000000004, -168.28, 3.8200000000000003]);

/// A map projection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Projection {
    /// Transverse Mercator (EPSG method 9807) with the latitude of origin
    /// at the equator: central meridian (degrees), scale factor, false
    /// easting and northing (metres).
    TransverseMercator { cm: f64, k0: f64, fe: f64, fn_: f64 },
    /// American Polyconic (EPSG method 9818), latitude of origin at the
    /// equator: central meridian (degrees), false easting and northing.
    Polyconic { cm: f64, fe: f64, fn_: f64 },
    /// Popular Visualisation Pseudo Mercator (EPSG method 1024), on the
    /// sphere of the ellipsoid's semi-major axis.
    PseudoMercator,
}

/// A coordinate reference system R5 can sample a raster in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Crs {
    /// The EPSG code.
    pub(crate) code: u32,
    pub(crate) datum: Datum,
    pub(crate) projection: Option<Projection>,
    /// GeoTools appends a projective transform that is the identity up to
    /// rounding (`x' = (x + 2^-33) / (1 + 2^-52)`, `y' = (y + 2^-29) /
    /// (1 + 2^-52)`) to WGS 84 / UTM south zones: its concatenation of the
    /// false northing does not cancel exactly.
    pub(crate) residual: bool,
    /// The name, for messages.
    pub(crate) name: &'static str,
}

const UTM_K0: f64 = 0.9996;

fn utm(code: u32, datum: Datum, zone: u32, south: bool, residual: bool, name: &'static str) -> Crs {
    Crs {
        code,
        datum,
        projection: Some(Projection::TransverseMercator {
            cm: f64::from(zone as i32 * 6 - 183),
            k0: UTM_K0,
            fe: 500_000.0,
            fn_: if south { 10_000_000.0 } else { 0.0 },
        }),
        residual,
        name,
    }
}

/// The system of EPSG `code` among those supported (whose transform has
/// been compared with GeoTools'), or `None`.
pub(crate) fn lookup(code: u32) -> Option<Crs> {
    let geographic = |datum, name| Crs {
        code,
        datum,
        projection: None,
        residual: false,
        name,
    };
    Some(match code {
        4326 => geographic(Datum::Wgs84, "WGS 84"),
        4674 => geographic(SIRGAS2000, "SIRGAS 2000"),
        4618 => geographic(SAD69, "SAD69"),
        4225 => geographic(CORREGO_ALEGRE, "Corrego Alegre 1970-72"),
        3857 => Crs {
            code,
            datum: Datum::Wgs84,
            projection: Some(Projection::PseudoMercator),
            residual: false,
            name: "WGS 84 / Pseudo-Mercator",
        },
        5880 => Crs {
            code,
            datum: SIRGAS2000,
            projection: Some(Projection::Polyconic {
                cm: -54.0,
                fe: 5_000_000.0,
                fn_: 10_000_000.0,
            }),
            residual: false,
            name: "SIRGAS 2000 / Brazil Polyconic",
        },
        // WGS 84 / UTM zones 1N-60N and 1S-60S
        32601..=32660 => utm(
            code,
            Datum::Wgs84,
            code - 32600,
            false,
            false,
            "WGS 84 / UTM",
        ),
        32701..=32760 => utm(code, Datum::Wgs84, code - 32700, true, true, "WGS 84 / UTM"),
        // SIRGAS 2000 / UTM zones 11N-22N and 17S-25S
        31965..=31976 => utm(
            code,
            SIRGAS2000,
            code - 31954,
            false,
            false,
            "SIRGAS 2000 / UTM",
        ),
        31977..=31985 => utm(
            code,
            SIRGAS2000,
            code - 31960,
            true,
            false,
            "SIRGAS 2000 / UTM",
        ),
        6210 | 6211 => utm(
            code,
            SIRGAS2000,
            code - 6187,
            false,
            false,
            "SIRGAS 2000 / UTM",
        ),
        5396 => utm(code, SIRGAS2000, 26, true, false, "SIRGAS 2000 / UTM"),
        // SAD69 / UTM zones 18N-22N and 17S-25S
        29168..=29172 => utm(code, SAD69, code - 29150, false, false, "SAD69 / UTM"),
        29187..=29195 => utm(code, SAD69, code - 29170, true, false, "SAD69 / UTM"),
        // Corrego Alegre 1970-72 / UTM zones 21S-25S
        22521..=22525 => utm(
            code,
            CORREGO_ALEGRE,
            code - 22500,
            true,
            false,
            "Corrego Alegre 1970-72 / UTM",
        ),
        _ => return None,
    })
}

/// `e^2 = (a^2 - b^2) / a^2`.
fn e2(e: Ellipsoid) -> f64 {
    (e.a * e.a - e.b * e.b) / (e.a * e.a)
}

/// Longitude and latitude in degrees on ellipsoid `e` (height 0) to
/// geocentric coordinates.
pub(crate) fn to_geocentric(e: Ellipsoid, lon: f64, lat: f64) -> [f64; 3] {
    let l = to_radians(lon);
    let p = to_radians(lat);
    let e2 = e2(e);
    let sin_p = fdlibm::sin(p);
    let cos_p = fdlibm::cos(p);
    let rn = e.a / (1.0 - e2 * (sin_p * sin_p)).sqrt();
    let x = rn * cos_p * fdlibm::cos(l);
    let y = rn * cos_p * fdlibm::sin(l);
    let z = rn * (1.0 - e2) * sin_p;
    [x, y, z]
}

/// Geocentric coordinates to longitude and latitude in degrees on
/// ellipsoid `e` (Toms' method).
pub(crate) fn from_geocentric(e: Ellipsoid, c: [f64; 3]) -> (f64, f64) {
    const AD_C: f64 = 1.0026;
    let [x, y, z] = c;
    let e2 = e2(e);
    let ep2 = (e.a * e.a - e.b * e.b) / (e.b * e.b);
    let lon = fdlibm::atan2(y, x);
    let w2 = x * x + y * y;
    let w = w2.sqrt();
    let t0 = z * AD_C;
    let s0 = (t0 * t0 + w2).sqrt();
    let sin_b0 = t0 / s0;
    let cos_b0 = w / s0;
    let t1 = z + (e.b * ep2) * (sin_b0 * sin_b0 * sin_b0);
    let sum = w - (e.a * e2) * (cos_b0 * cos_b0 * cos_b0);
    let s1 = (t1 * t1 + sum * sum).sqrt();
    let sin_p1 = t1 / s1;
    let cos_p1 = sum / s1;
    let lat = fdlibm::atan(sin_p1 / cos_p1);
    (to_degrees(lon), to_degrees(lat))
}

/// Transverse Mercator on ellipsoid `e`, from degrees.
pub(crate) fn transverse_mercator(
    e: Ellipsoid,
    cm: f64,
    k0: f64,
    fe: f64,
    fn_: f64,
    lon: f64,
    lat: f64,
) -> (f64, f64) {
    const FC1: f64 = 1.0;
    const FC2: f64 = 0.5;
    const FC3: f64 = 0.16666666666666666666;
    const FC4: f64 = 0.08333333333333333333;
    const FC5: f64 = 0.05;
    const FC6: f64 = 0.03333333333333333333;
    const FC7: f64 = 0.02380952380952380952;
    const FC8: f64 = 0.01785714285714285714;
    // (the map projections derive it from the axis ratio)
    let es = 1.0 - (e.b * e.b) / (e.a * e.a);
    let esp = es / (1.0 - es);
    let x = roll(to_radians(lon) - to_radians(cm));
    let y = to_radians(lat);
    let sinphi = fdlibm::sin(y);
    let cosphi = fdlibm::cos(y);
    let mut t = if cosphi.abs() > 1e-10 {
        sinphi / cosphi
    } else {
        0.0
    };
    t *= t;
    let mut al = cosphi * x;
    let als = al * al;
    al /= (1.0 - es * sinphi * sinphi).sqrt();
    let n = esp * cosphi * cosphi;
    let my = mlfn(es, y, sinphi, cosphi)
        + sinphi
            * al
            * x
            * FC2
            * (1.0
                + FC4
                    * als
                    * (5.0 - t
                        + n * (9.0 + 4.0 * n)
                        + FC6
                            * als
                            * (61.0
                                + t * (t - 58.0)
                                + n * (270.0 - 330.0 * t)
                                + FC8 * als * (1385.0 + t * (t * (543.0 - t) - 3111.0)))));
    let mx = al
        * (FC1
            + FC3
                * als
                * (1.0 - t
                    + n
                    + FC5
                        * als
                        * (5.0
                            + t * (t - 18.0)
                            + n * (14.0 - 58.0 * t)
                            + FC7 * als * (61.0 + t * (t * (179.0 - t) - 479.0)))));
    let scale = k0 * e.a;
    (scale * mx + fe, scale * my + fn_)
}

/// `MapProjection.rollLongitude`: an angle in radians brought into
/// [-pi, pi].
fn roll(x: f64) -> f64 {
    const TWO_PI: f64 = 2.0 * std::f64::consts::PI;
    x - TWO_PI * (x / TWO_PI + 0.5).floor()
}

/// The meridian distance on the unit ellipsoid of eccentricity squared
/// `es` (Snyder's series, PROJ.4's `pj_mlfn`).
fn mlfn(es: f64, phi: f64, sphi: f64, cphi: f64) -> f64 {
    const C00: f64 = 1.0;
    const C02: f64 = 0.25;
    const C04: f64 = 0.046875;
    const C06: f64 = 0.01953125;
    const C08: f64 = 0.01068115234375;
    const C22: f64 = 0.75;
    const C44: f64 = 0.46875;
    const C46: f64 = 0.01302083333333333333;
    const C48: f64 = 0.00712076822916666666;
    const C66: f64 = 0.36458333333333333333;
    const C68: f64 = 0.00569661458333333333;
    const C88: f64 = 0.3076171875;
    let en0 = C00 - es * (C02 + es * (C04 + es * (C06 + es * C08)));
    let en1 = es * (C22 - es * (C04 + es * (C06 + es * C08)));
    let mut t = es * es;
    let en2 = t * (C44 - es * (C46 + es * C48));
    t *= es;
    let en3 = t * (C66 - es * C68);
    let en4 = t * es * C88;
    let c = cphi * sphi;
    let s = sphi * sphi;
    en0 * phi - c * (en1 + s * (en2 + s * (en3 + s * en4)))
}

/// The American Polyconic on ellipsoid `e` (Snyder, equations 18-1 to
/// 18-3, as PROJ.4's `poly`), from degrees, the latitude of origin at the
/// equator.
fn polyconic(e: Ellipsoid, cm: f64, fe: f64, fn_: f64, lon: f64, lat: f64) -> (f64, f64) {
    const TOL: f64 = 1e-10;
    let es = 1.0 - (e.b * e.b) / (e.a * e.a);
    let lam = roll(to_radians(lon) - to_radians(cm));
    let phi = to_radians(lat);
    let (x, y) = if phi.abs() <= TOL {
        (lam, 0.0)
    } else {
        let sp = fdlibm::sin(phi);
        let cp = fdlibm::cos(phi);
        let ms = if cp.abs() > TOL {
            cp / (1.0 - es * (sp * sp)).sqrt() / sp
        } else {
            0.0
        };
        let l = lam * sp;
        (
            ms * fdlibm::sin(l),
            mlfn(es, phi, sp, cp) + ms * (1.0 - fdlibm::cos(l)),
        )
    };
    (e.a * x + fe, e.a * y + fn_)
}

/// The pseudo-Mercator, from degrees.
fn pseudo_mercator(a: f64, lon: f64, lat: f64) -> (f64, f64) {
    let x = to_radians(lon);
    let y = fdlibm::log(fdlibm::tan(
        std::f64::consts::FRAC_PI_4 + 0.5 * to_radians(lat),
    ));
    (a * x + 0.0, a * y + 0.0)
}

/// The systems [`lookup`] knows, for messages.
pub(crate) const SUPPORTED: &str = "EPSG:4326 (WGS 84), 4674 (SIRGAS 2000), 4618 (SAD69), \
     4225 (Corrego Alegre 1970-72), 32601-32660 and 32701-32760 (WGS 84 / UTM), 31965-31985, \
     6210, 6211 and 5396 (SIRGAS 2000 / UTM), 5880 (SIRGAS 2000 / Brazil Polyconic), \
     29168-29172 and 29187-29195 (SAD69 / UTM), 22521-22525 (Corrego Alegre 1970-72 / UTM) \
     and 3857 (WGS 84 / Pseudo-Mercator)";

/// How far, in metres, a point projected by GeoTools may be from where
/// [`Crs::apply`] puts it because of the JVM's own sines and cosines:
/// 5e-8 m, more than six times the largest difference measured between
/// GeoTools on Java 21 (arm64, with the JVM's intrinsics) and these
/// transforms, 7.5e-9 m over 1.5 million points per system (those within 8
/// degrees of the central meridian).
const MARGIN_M: f64 = 5e-8;
/// The same for a geographic system, in degrees: seven times the largest
/// difference measured (2.8e-14 degrees, 3 nm).
const MARGIN_DEG: f64 = 2e-13;

impl Crs {
    /// `"EPSG:31982 (SIRGAS 2000 / UTM zone 22S)"`.
    pub(crate) fn label(&self) -> String {
        match self.projection {
            Some(Projection::TransverseMercator { cm, fn_, .. }) if self.name.ends_with("UTM") => {
                format!(
                    "EPSG:{} ({} zone {}{})",
                    self.code,
                    self.name,
                    (cm as i32 + 183) / 6,
                    if fn_ > 0.0 { 'S' } else { 'N' }
                )
            }
            _ => format!("EPSG:{} ({})", self.code, self.name),
        }
    }

    /// How far GeoTools' result may be from [`Crs::apply`]'s on some JVM
    /// (see the module notes), in the system's units; `None` for WGS 84
    /// longitude and latitude, where nothing is computed. A sample this
    /// close to a boundary between pixels of different values is read as
    /// `StrictMath` reads it and counted for the build's warning.
    pub(crate) fn margin(&self) -> Option<f64> {
        match (self.datum, self.projection) {
            (Datum::Wgs84, None) => None,
            (_, None) => Some(MARGIN_DEG),
            (_, Some(_)) => Some(MARGIN_M),
        }
    }

    /// WGS 84 longitude and latitude (degrees) in this system.
    pub(crate) fn apply(&self, lon: f64, lat: f64) -> (f64, f64) {
        let (mut lon, mut lat) = (lon, lat);
        match self.datum {
            Datum::Wgs84 => {}
            Datum::Ellipsoid(e) => {
                (lon, lat) = from_geocentric(e, to_geocentric(WGS84, lon, lat));
            }
            Datum::Shift(e, d) => {
                let c = to_geocentric(WGS84, lon, lat);
                (lon, lat) = from_geocentric(e, [c[0] + d[0], c[1] + d[1], c[2] + d[2]]);
            }
        }
        let e = self.datum.ellipsoid();
        let (mut x, mut y) = match self.projection {
            None => (lon, lat),
            Some(Projection::TransverseMercator { cm, k0, fe, fn_ }) => {
                transverse_mercator(e, cm, k0, fe, fn_, lon, lat)
            }
            Some(Projection::Polyconic { cm, fe, fn_ }) => polyconic(e, cm, fe, fn_, lon, lat),
            Some(Projection::PseudoMercator) => pseudo_mercator(e.a, lon, lat),
        };
        if self.residual {
            let w = 1.0 + f64::EPSILON;
            x = (x + f64::from_bits(0x3de0000000000000)) / w;
            y = (y + f64::from_bits(0x3e20000000000000)) / w;
        }
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Points transformed by GeoTools 25.2 (r5r 2.5.0's R5 jar) on a JVM
    /// whose sines and cosines are `StrictMath`'s: `(code, lon, lat, x, y)`
    /// as bits (Brazil, Porto Alegre's streets, the equator).
    #[rustfmt::skip]
    const GEOTOOLS: &[(u32, u64, u64, u64, u64)] = &[
            (3857, 0xc03e10fee24de232, 0x40133c2a20d48568, 0xc149890f91ad291a, 0x41205b03516678ad),
            (3857, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0xc1524087b4b9c357, 0x3e7e087d36ffffb5),
            (3857, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0xc155bc02e3792fb3, 0xc14abd94c5492c75),
            (3857, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0xc15c4eb0e1cc1458, 0xc04dfd3a72f3b494),
            (4225, 0xc03e10fee24de232, 0x40133c2a20d48568, 0xc03e1117ef7d35ad, 0x40133c2306f70f62),
            (4225, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0xc0457d9a5d070464, 0x3f021cbf58b5d5c2),
            (4225, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0xc049973720a65fcc, 0xc03e02a34523325f),
            (4225, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0xc050aa3365534d0d, 0xbf4085e689903e3c),
            (4618, 0xc03e10fee24de232, 0x40133c2a20d48568, 0xc03e10e7cbfb7cea, 0x40133c9c258fa553),
            (4618, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0xc0457d9144354630, 0x3f3ed1d06c3f088c),
            (4618, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0xc0499737def59587, 0xc03e02a0c1f2e23e),
            (4618, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0xc050aa3cd0324ba2, 0xbf11f675bd1154c6),
            (4674, 0xc03e10fee24de232, 0x40133c2a20d48568, 0xc03e10fee24de230, 0x40133c2a20da92a1),
            (4674, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0xc0457d9f79b8e567, 0x3d719511179d1c9f),
            (4674, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0xc0499749d388a8b1, 0xc03e02c379df197d),
            (4674, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0xc050aa456d7f37b5, 0xbf41a7bcb409cf92),
            (5396, 0xc03e10fee24de232, 0x40133c2a20d48568, 0x4103825bca4b2d74, 0x416416b800c477d5),
            (5396, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0xc133ddfedd23a68d, 0x416312d00000003e),
            (5396, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0xc13c7afa58af6ab0, 0x41587df4e35f5152),
            (5396, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0xc15077bce78109d4, 0x416312c64fe7232f),
            (5880, 0xc03e10fee24de232, 0x40133c2a20d48568, 0x415d3304ca4bc40a, 0x41642d252d6e3ea7),
            (5880, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0x4157c0a56b74b6f2, 0x416312d000000000),
            (5880, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0x41541c49ce552e18, 0x415976e4802a1dc6),
            (5880, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0x414b64f87cc6dba1, 0x416312c85eaf97a7),
            (6210, 0xc03e10fee24de232, 0x40133c2a20d48568, 0x414097c30f4c1984, 0x4120c9ff05ca08b3),
            (6210, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0x41261d6e588c0846, 0x3e7dac0a5feeafb8),
            (6210, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0xc0f79ef50bdbb240, 0xc14973dd84a599bc),
            (6210, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0xc13e104479c22e3c, 0xc0500725cd7a7b32),
            (22522, 0xc03e10fee24de232, 0x40133c2a20d48568, 0x4145edde9cf05ab2, 0x416428ba9d0d5048),
            (22522, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0x41354a41eba6e814, 0x416312d07b67d76e),
            (22522, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0x411d73315b552963, 0x41597b72afd33145),
            (22522, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0xc1334cbcd451c5d2, 0x416312c8c33fa6da),
            (29192, 0xc03e10fee24de232, 0x40133c2a20d48568, 0x4145edddf8765aea, 0x416428c11c96d14f),
            (29192, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0x41354a41088f0c32, 0x416312d68fde4b83),
            (29192, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0x411d732b5373acd9, 0x41597b7cf326155e),
            (29192, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0xc1334cbf898d742f, 0x416312cf043a7189),
            (31976, 0xc03e10fee24de232, 0x40133c2a20d48568, 0x4145edc59c245dbf, 0x41215ea44625bcd7),
            (31976, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0x41354a0d12bffd3e, 0x3e7df2c2efaba170),
            (31976, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0x411d7258940226ee, 0xc149545dbe2ed060),
            (31976, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0xc1334cf5f292a464, 0xc04eeec31afb7af0),
            (31982, 0xc03e10fee24de232, 0x40133c2a20d48568, 0x4145edc59c245dbf, 0x416428ba44625bcd),
            (31982, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0x41354a0d12bffd3e, 0x416312d00000003c),
            (31982, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0x411d7258940226ee, 0x41597b7120e897d0),
            (31982, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0xc1334cf5f292a464, 0x416312c8444f3941),
            (32601, 0xc03e10fee24de232, 0x40133c2a20d48568, 0x419b730852b0e79f, 0x41894789b9384856),
            (32601, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0x419281434f04774e, 0x3ed9b4f0e6ac373e),
            (32601, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0x415c03bfe8878344, 0xc18138438384a117),
            (32601, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0x4180c4ee453f8a3c, 0xc08f48580053ba14),
            (32722, 0xc03e10fee24de232, 0x40133c2a20d48568, 0x4145edc59c245734, 0x416428ba446228c5),
            (32722, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0x41354a0d12bffb9b, 0x416312d00000003c),
            (32722, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0x411d72589400e160, 0x41597b712119d10d),
            (32722, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0xc1334cf5f29297a9, 0x416312c8444f3941),
            (32760, 0xc03e10fee24de232, 0x40133c2a20d48568, 0x41a0d4ff9aa7dfc3, 0x41930ffa0e0b28e7),
            (32760, 0xc0457d9f79b8e566, 0x3d719511179a9f0d, 0x4196c1d0648ff5ef, 0x416312d000001135),
            (32760, 0xc0499749d388a8b1, 0xc03e02c37966f42b, 0x41529a4ffdf67020, 0xc180a418f8415508),
            (32760, 0xc050aa456d7f37b5, 0xbf41a7bcb4074f5c, 0x41845f6ffecbec80, 0x416312260779b4e9),
    ];

    #[test]
    fn transforms_equal_geotools_bit_for_bit() {
        for &(code, lon, lat, x, y) in GEOTOOLS {
            let c = lookup(code).unwrap();
            let (gx, gy) = c.apply(f64::from_bits(lon), f64::from_bits(lat));
            assert_eq!(
                (gx.to_bits(), gy.to_bits()),
                (x, y),
                "EPSG:{code} at {lon:x},{lat:x}"
            );
        }
    }

    #[test]
    fn parameters_have_geotools_bits() {
        let bits = |e: Ellipsoid| (e.a.to_bits(), e.b.to_bits());
        assert_eq!(bits(WGS84), (0x415854a640000000, 0x41583fc4141c97d0));
        assert_eq!(bits(GRS80), (0x415854a640000000, 0x41583fc4141ae027));
        assert_eq!(bits(GRS67M), (0x415854ac00000000, 0x41583fc9ae074bbf));
        assert_eq!(bits(INTL24), (0x415854e500000000, 0x41583febfc8d5c3b));
        let Datum::Shift(_, d) = CORREGO_ALEGRE else {
            unreachable!()
        };
        assert_eq!(
            d.map(f64::to_bits),
            [0x4069c1999999999b, 0xc06508f5c28f5c29, 0x400e8f5c28f5c290]
        );
    }

    #[test]
    fn the_table() {
        // the zones as GeoTools' database has them
        let cm = |code| match lookup(code).unwrap().projection {
            Some(Projection::TransverseMercator { cm, fn_, .. }) => (cm, fn_),
            p => panic!("{p:?}"),
        };
        assert_eq!(cm(32601), (-177.0, 0.0));
        assert_eq!(cm(32760), (177.0, 10_000_000.0));
        assert_eq!(cm(31965), (-117.0, 0.0));
        assert_eq!(cm(31977), (-81.0, 10_000_000.0));
        assert_eq!(cm(31985), (-33.0, 10_000_000.0));
        assert_eq!(cm(6210), (-45.0, 0.0));
        assert_eq!(cm(6211), (-39.0, 0.0));
        assert_eq!(cm(5396), (-27.0, 10_000_000.0));
        assert_eq!(cm(29168), (-75.0, 0.0));
        assert_eq!(cm(29195), (-33.0, 10_000_000.0));
        assert_eq!(cm(22521), (-57.0, 10_000_000.0));
        assert!(lookup(32761).is_none() && lookup(29173).is_none() && lookup(2193).is_none());
        assert!(lookup(32722).unwrap().residual && !lookup(32622).unwrap().residual);
        assert_eq!(
            lookup(31982).unwrap().label(),
            "EPSG:31982 (SIRGAS 2000 / UTM zone 22S)"
        );
        assert_eq!(lookup(4674).unwrap().label(), "EPSG:4674 (SIRGAS 2000)");
        assert_eq!(lookup(4326).unwrap().margin(), None);
        assert_eq!(lookup(4674).unwrap().margin(), Some(MARGIN_DEG));
        assert_eq!(lookup(3857).unwrap().margin(), Some(MARGIN_M));
        // WGS 84 is the identity
        let w = lookup(4326).unwrap();
        assert_eq!(w.apply(-51.25, -30.01), (-51.25, -30.01));
    }
}
