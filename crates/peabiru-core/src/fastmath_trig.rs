//! commons-math3 3.0 `FastMath.toRadians`, `sin`, `cos`, `asin` and `pow`,
//! bit for bit, as bundled in the R5 7.6 jar (`r5-v7.6-all.jar`), for
//! R5's haversine distance (`GeometryUtils.distance`, which
//! `find_snap()` reports). They are not correctly rounded, so the platform
//! libm differs from them in the last bit for some arguments, and the
//! haversine formula, which subtracts nearby angles, turns that into
//! differences of about 1e-9 metres.
//!
//! The lookup tables below are `FastMath.SINE_TABLE_A/B`,
//! `COSINE_TABLE_A/B`, `TANGENT_TABLE_A/B`, `EIGHTHS`, `LN_HI_PREC_COEF`
//! and `FastMath$lnMant.LN_MANT`, dumped from that jar
//! by reflection as raw IEEE-754 bits. Arguments beyond 3,294,198 radians,
//! which FastMath reduces with the Payne-Hanek method, are not ported (no
//! longitude or latitude in radians comes near them); `sin` and `cos` fall
//! back to the platform's there.
//!
//! This file is a translation to Rust, by the peabiru authors, of part of
//! `org.apache.commons.math3.util.FastMath` of Apache Commons Math 3.0
//! (Copyright 2001-2012 The Apache Software Foundation), licensed under the
//! Apache License, Version 2.0, ported from the bytecode in that jar. The
//! licence and the notices are in `LICENSE-THIRD-PARTY`.

// The constants are FastMath's, digit for digit.
#![allow(clippy::approx_constant, clippy::excessive_precision)]

use crate::fastmath::exp_extra;

/// 2^30, for splitting a double in a high and a low part.
const HEX_40000000: f64 = 1_073_741_824.0;
const TWO_POWER_52: f64 = 4.503_599_627_370_496E15;
const LN_2_A: f64 = 0.693_147_063_255_310_1;
const LN_2_B: f64 = 1.173_046_352_508_234_8E-7;
const PI_2_A: f64 = 1.570_796_326_794_896_6;
const PI_2_B: f64 = 6.123_233_995_736_766E-17;

#[inline]
fn t(table: &[u64], i: usize) -> f64 {
    f64::from_bits(table[i])
}

/// `FastMath.doubleHighPart`: the value with its 30 low bits cleared
/// (subnormals unchanged).
fn double_high_part(d: f64) -> f64 {
    const SAFE_MIN: f64 = 2.225_073_858_507_201_4E-308;
    if d > -SAFE_MIN && d < SAFE_MIN {
        return d;
    }
    f64::from_bits(d.to_bits() & 0xFFFF_FFFF_C000_0000)
}

/// `FastMath.toRadians(x)`.
pub fn to_radians(x: f64) -> f64 {
    if x.is_infinite() || x == 0.0 {
        return x;
    }
    const FACTA: f64 = 0.017_453_290_522_098_54;
    const FACTB: f64 = 1.997_844_754_509_471E-9;
    let xa = double_high_part(x);
    let xb = x - xa;
    let result = xb * FACTB + xb * FACTA + xa * FACTB + xa * FACTA;
    if result == 0.0 {
        return result * x;
    }
    result
}

fn poly_sine(x: f64) -> f64 {
    let x2 = x * x;
    let mut p = 2.755_381_745_227_221_7E-6;
    p = p * x2 + -1.984_126_965_958_650_5E-4;
    p = p * x2 + 0.008_333_333_333_329_196;
    p = p * x2 + -0.166_666_666_666_666_66;
    p * x2 * x
}

fn poly_cosine(x: f64) -> f64 {
    let x2 = x * x;
    let mut p = 2.479_773_539_153_719E-5;
    p = p * x2 + -0.001_388_888_868_903_988_3;
    p = p * x2 + 0.041_666_666_666_621_166;
    p = p * x2 + -0.499_999_999_999_999_94;
    p * x2
}

/// `FastMath.sinQ`: sine of `xa + xb` in `[0, pi/2]`.
fn sin_q(xa: f64, xb: f64) -> f64 {
    let idx = (xa * 8.0 + 0.5) as i32 as usize;
    let epsilon = xa - t(&EIGHTHS, idx);
    let sint_a = t(&SINE_TABLE_A, idx);
    let sint_b = t(&SINE_TABLE_B, idx);
    let cost_a = t(&COSINE_TABLE_A, idx);
    let cost_b = t(&COSINE_TABLE_B, idx);
    let mut sin_eps_a = epsilon;
    let mut sin_eps_b = poly_sine(epsilon);
    let cos_eps_b = poly_cosine(epsilon);
    let temp = sin_eps_a * HEX_40000000;
    let temp2 = (sin_eps_a + temp) - temp;
    sin_eps_b += sin_eps_a - temp2;
    sin_eps_a = temp2;

    let mut a = 0.0f64;
    let mut b = 0.0f64;
    let mut tt = sint_a;
    let mut c = a + tt;
    let mut d = -(c - a - tt);
    a = c;
    b += d;
    tt = cost_a * sin_eps_a;
    c = a + tt;
    d = -(c - a - tt);
    a = c;
    b += d;
    b = b + sint_a * cos_eps_b + cost_a * sin_eps_b;
    b = b + sint_b + cost_b * sin_eps_a + sint_b * cos_eps_b + cost_b * sin_eps_b;
    if xb != 0.0 {
        tt = ((cost_a + cost_b) * (1.0 + cos_eps_b) - (sint_a + sint_b) * (sin_eps_a + sin_eps_b))
            * xb;
        c = a + tt;
        d = -(c - a - tt);
        a = c;
        b += d;
    }
    a + b
}

/// `FastMath.cosQ`: cosine of `xa + xb` in `[0, pi/2]`.
fn cos_q(xa: f64, xb: f64) -> f64 {
    let a = PI_2_A - xa;
    let mut b = -(a - PI_2_A + xa);
    b += PI_2_B - xb;
    sin_q(a, b)
}

/// The Cody-Waite reduction of `sin` and `cos` for `xa` in
/// `(pi/2, 3294198]`: quadrant, and the remainder as `(a, b)`.
fn cody_waite(xa: f64) -> (i32, f64, f64) {
    let mut k = (xa * 0.636_619_772_367_581_4) as i32;
    loop {
        let mut a = f64::from(-k) * 1.570_796_251_296_997;
        let mut rem_a = xa + a;
        let mut rem_b = -(rem_a - xa - a);
        a = f64::from(-k) * 7.549_789_948_768_648E-8;
        let mut b = rem_a;
        rem_a = a + b;
        rem_b += -(rem_a - b - a);
        a = f64::from(-k) * 6.123_233_995_736_766E-17;
        b = rem_a;
        rem_a = a + b;
        rem_b += -(rem_a - b - a);
        if rem_a > 0.0 {
            return (k & 3, rem_a, rem_b);
        }
        k -= 1;
    }
}

/// `FastMath.sin(x)`.
pub fn sin(x: f64) -> f64 {
    let mut negative = false;
    let mut quadrant = 0;
    let mut xa = x;
    let mut xb = 0.0;
    if x < 0.0 {
        negative = true;
        xa = -xa;
    }
    if xa == 0.0 {
        return if x.to_bits() >> 63 == 1 { -0.0 } else { 0.0 };
    }
    if xa.is_nan() || xa == f64::INFINITY {
        return f64::NAN;
    }
    if xa > 3_294_198.0 {
        return x.sin();
    } else if xa > PI_2_A {
        (quadrant, xa, xb) = cody_waite(xa);
    }
    if negative {
        quadrant ^= 2;
    }
    match quadrant {
        0 => sin_q(xa, xb),
        1 => cos_q(xa, xb),
        2 => -sin_q(xa, xb),
        _ => -cos_q(xa, xb),
    }
}

/// `FastMath.cos(x)`.
pub fn cos(x: f64) -> f64 {
    let mut quadrant = 0;
    let mut xa = x;
    if x < 0.0 {
        xa = -xa;
    }
    if xa.is_nan() || xa == f64::INFINITY {
        return f64::NAN;
    }
    let mut xb = 0.0;
    if xa > 3_294_198.0 {
        return x.cos();
    } else if xa > PI_2_A {
        (quadrant, xa, xb) = cody_waite(xa);
    }
    match quadrant {
        0 => cos_q(xa, xb),
        1 => -sin_q(xa, xb),
        2 => -cos_q(xa, xb),
        _ => sin_q(xa, xb),
    }
}

/// `FastMath.atan(xa, xb, leftPlane)`: the arctangent of `xa + xb`.
fn atan(mut xa: f64, mut xb: f64, left_plane: bool) -> f64 {
    let mut negate = false;
    if xa == 0.0 {
        return if left_plane {
            std::f64::consts::PI.copysign(xa)
        } else {
            xa
        };
    }
    if xa < 0.0 {
        xa = -xa;
        xb = -xb;
        negate = true;
    }
    if xa > 1.633_123_935_319_537E16 {
        return if negate ^ left_plane { -PI_2_A } else { PI_2_A };
    }
    let idx = if xa < 1.0 {
        ((-1.716_814_692_820_413_5 * xa * xa + 8.0) * xa + 0.5) as i32
    } else {
        let one_over_xa = 1.0 / xa;
        (-((-1.716_814_692_820_413_5 * one_over_xa * one_over_xa + 8.0) * one_over_xa) + 13.07)
            as i32
    } as usize;
    let mut eps_a = xa - t(&TANGENT_TABLE_A, idx);
    let mut eps_b = -(eps_a - xa + t(&TANGENT_TABLE_A, idx));
    eps_b += xb - t(&TANGENT_TABLE_B, idx);
    let mut temp = eps_a + eps_b;
    eps_b = -(temp - eps_a - eps_b);
    eps_a = temp;

    temp = xa * HEX_40000000;
    let mut ya = xa + temp - temp;
    let mut yb = xb + xa - ya;
    xa = ya;
    xb += yb;

    if idx == 0 {
        let denom = 1.0 / (1.0 + (xa + xb) * (t(&TANGENT_TABLE_A, idx) + t(&TANGENT_TABLE_B, idx)));
        ya = eps_a * denom;
        yb = eps_b * denom;
    } else {
        let mut temp2 = xa * t(&TANGENT_TABLE_A, idx);
        let mut za = 1.0 + temp2;
        let mut zb = -(za - 1.0 - temp2);
        temp2 = xb * t(&TANGENT_TABLE_A, idx) + xa * t(&TANGENT_TABLE_B, idx);
        temp = za + temp2;
        zb += -(temp - za - temp2);
        za = temp;
        zb += xb * t(&TANGENT_TABLE_B, idx);
        ya = eps_a / za;
        temp = ya * HEX_40000000;
        let yaa = (ya + temp) - temp;
        let yab = ya - yaa;
        temp = za * HEX_40000000;
        let zaa = (za + temp) - temp;
        let zab = za - zaa;
        yb = (eps_a - yaa * zaa - yaa * zab - yab * zaa - yab * zab) / za;
        yb += -eps_a * zb / za / za;
        yb += eps_b / za;
    }

    eps_a = ya;
    eps_b = yb;
    let eps_a2 = eps_a * eps_a;
    yb = 0.074_908_222_888_644_72;
    yb = yb * eps_a2 + -0.090_884_508_661_851_92;
    yb = yb * eps_a2 + 0.111_110_959_423_133_05;
    yb = yb * eps_a2 + -0.142_857_142_367_918_2;
    yb = yb * eps_a2 + 0.199_999_999_999_235_82;
    yb = yb * eps_a2 + -0.333_333_333_333_332_87;
    yb = yb * eps_a2 * eps_a;
    ya = eps_a;
    temp = ya + yb;
    yb = -(temp - ya - yb);
    ya = temp;
    yb += eps_b / (1.0 + eps_a * eps_a);

    let eighths = t(&EIGHTHS, idx);
    let mut za = eighths + ya;
    let mut zb = -(za - eighths - ya);
    temp = za + yb;
    zb += -(temp - za - yb);
    za = temp;
    let mut result = za + zb;
    let resultb = -(result - za - zb);
    if left_plane {
        const PIA: f64 = std::f64::consts::PI;
        const PIB: f64 = 1.224_646_799_147_353_2E-16;
        za = PIA - result;
        zb = -(za - PIA + result);
        zb += PIB - resultb;
        result = za + zb;
    }
    if negate ^ left_plane {
        result = -result;
    }
    result
}

/// `FastMath.asin(x)`.
pub fn asin(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    if x > 1.0 || x < -1.0 {
        return f64::NAN;
    }
    if x == 1.0 {
        return PI_2_A;
    }
    if x == -1.0 {
        return -PI_2_A;
    }
    if x == 0.0 {
        return x;
    }
    let mut temp = x * HEX_40000000;
    let xa = x + temp - temp;
    let xb = x - xa;
    let mut ya = xa * xa;
    let mut yb = xa * xb * 2.0 + xb * xb;
    ya = -ya;
    yb = -yb;
    let mut za = 1.0 + ya;
    let mut zb = -(za - 1.0 - ya);
    temp = za + yb;
    zb += -(temp - za - yb);
    za = temp;
    let y = za.sqrt();
    temp = y * HEX_40000000;
    ya = y + temp - temp;
    yb = y - ya;
    yb += (za - ya * ya - 2.0 * ya * yb - yb * yb) / (2.0 * y);
    let dx = zb / (2.0 * y);
    let r = x / y;
    temp = r * HEX_40000000;
    let mut ra = r + temp - temp;
    let mut rb = r - ra;
    rb += (x - ra * ya - ra * yb - rb * ya - rb * yb) / y;
    rb += -x * dx / y / y;
    temp = ra + rb;
    rb = -(temp - ra - rb);
    ra = temp;
    atan(ra, rb, false)
}

/// `FastMath.log(x, hiPrec)` with `hiPrec` given: `ln(x)` as a high and a
/// low part, for a positive normal or subnormal `x` (the caller handles
/// the other cases).
fn log_hi_prec(x: f64) -> (f64, f64) {
    let mut bits = x.to_bits() as i64;
    let mut exp = (bits >> 52) as i32 - 1023;
    if bits & 0x7ff0_0000_0000_0000 == 0 {
        bits <<= 1;
        while bits & 0x0010_0000_0000_0000 == 0 {
            exp -= 1;
            bits <<= 1;
        }
    }
    let lnm = LN_MANT[((bits & 0x000f_fc00_0000_0000) >> 42) as usize];
    let (lnm0, lnm1) = (f64::from_bits(lnm[0]), f64::from_bits(lnm[1]));
    let epsilon =
        (bits & 0x3ff_ffff_ffff) as f64 / (TWO_POWER_52 + (bits & 0x000f_fc00_0000_0000) as f64);

    let mut tmp = epsilon * HEX_40000000;
    let mut aa = epsilon + tmp - tmp;
    let mut ab = epsilon - aa;
    let xa = aa;
    let mut xb = ab;
    let numer = (bits & 0x3ff_ffff_ffff) as f64;
    let denom = TWO_POWER_52 + (bits & 0x000f_fc00_0000_0000) as f64;
    aa = numer - xa * denom - xb * denom;
    xb += aa / denom;

    let last = LN_HI_PREC_COEF[LN_HI_PREC_COEF.len() - 1];
    let mut ya = f64::from_bits(last[0]);
    let mut yb = f64::from_bits(last[1]);
    for c in LN_HI_PREC_COEF[..LN_HI_PREC_COEF.len() - 1].iter().rev() {
        aa = ya * xa;
        ab = ya * xb + yb * xa + yb * xb;
        tmp = aa * HEX_40000000;
        ya = aa + tmp - tmp;
        yb = aa - ya + ab;
        aa = ya + f64::from_bits(c[0]);
        ab = yb + f64::from_bits(c[1]);
        tmp = aa * HEX_40000000;
        ya = aa + tmp - tmp;
        yb = aa - ya + ab;
    }
    aa = ya * xa;
    ab = ya * xb + yb * xa + yb * xb;
    let lnza = aa + ab;
    let lnzb = -(lnza - aa - ab);

    let e = f64::from(exp);
    let mut a = LN_2_A * e;
    let mut b = 0.0;
    let mut c = a + lnm0;
    let mut d = -(c - a - lnm0);
    a = c;
    b += d;
    c = a + lnza;
    d = -(c - a - lnza);
    a = c;
    b += d;
    c = a + LN_2_B * e;
    d = -(c - a - LN_2_B * e);
    a = c;
    b += d;
    c = a + lnm1;
    d = -(c - a - lnm1);
    a = c;
    b += d;
    c = a + lnzb;
    d = -(c - a - lnzb);
    a = c;
    b += d;
    (a, b)
}

/// `FastMath.pow(x, y)` for the finite `x >= 0` and `y` the haversine
/// formula passes (`y = 2`); other arguments follow FastMath's special
/// cases where cheap and otherwise the platform's `powf`.
pub fn pow(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return 1.0;
    }
    if x.is_nan() {
        return x;
    }
    if x == 0.0 {
        let negative_zero = x.to_bits() >> 63 == 1;
        let odd_integer = y == (y as i64) as f64 && (y as i64) & 1 == 1;
        if negative_zero && odd_integer {
            return if y < 0.0 { f64::NEG_INFINITY } else { -0.0 };
        }
        return if y < 0.0 {
            f64::INFINITY
        } else if y > 0.0 {
            0.0
        } else {
            f64::NAN
        };
    }
    if !x.is_finite() || !y.is_finite() {
        return x.powf(y);
    }
    if x < 0.0 {
        if y >= TWO_POWER_52 || y <= -TWO_POWER_52 {
            return pow(-x, y);
        }
        if y == (y as i64) as f64 {
            return if (y as i64) & 1 == 0 {
                pow(-x, y)
            } else {
                -pow(-x, y)
            };
        }
        return f64::NAN;
    }
    let (ya, yb) = if y < 8e298 && y > -8e298 {
        let tmp1 = y * HEX_40000000;
        let ya = y + tmp1 - tmp1;
        (ya, y - ya)
    } else {
        let tmp1 = y * 9.313_225_746_154_785E-10;
        let tmp2 = tmp1 * 9.313_225_746_154_785E-10;
        let ya = (tmp1 + tmp2 - tmp1) * HEX_40000000 * HEX_40000000;
        (ya, y - ya)
    };
    let (mut lna, mut lnb) = log_hi_prec(x);
    if (lna + lnb).is_infinite() {
        return lna + lnb;
    }
    let tmp1 = lna * HEX_40000000;
    let tmp2 = lna + tmp1 - tmp1;
    lnb += lna - tmp2;
    lna = tmp2;
    let aa = lna * ya;
    let ab = lna * yb + lnb * ya + lnb * yb;
    lna = aa + ab;
    lnb = -(lna - aa - ab);
    let mut z = 1.0 / 120.0;
    z = z * lnb + (1.0 / 24.0);
    z = z * lnb + (1.0 / 6.0);
    z = z * lnb + 0.5;
    z = z * lnb + 1.0;
    z *= lnb;
    exp_extra(lna, z)
}

/// R5's `GeometryUtils.distance(lat0, lon0, lat1, lon1)`: haversine metres
/// on a sphere of radius 6,367,450 m, with commons-math's `FastMath`.
pub fn haversine_m(lat0: f64, lon0: f64, lat1: f64, lon1: f64) -> f64 {
    let theta0 = to_radians(lat0);
    let theta1 = to_radians(lat1);
    let lambda0 = to_radians(lon0);
    let lambda1 = to_radians(lon1);
    let theta_comb = (theta1 - theta0) / 2.0;
    let lambda_comb = (lambda1 - lambda0) / 2.0;
    let sin2theta = pow(sin(theta_comb), 2.0);
    let sin2lambda = pow(sin(lambda_comb), 2.0);
    let under = sin2theta + cos(theta0) * cos(theta1) * sin2lambda;
    2.0 * 6_367_450.0 * asin(under.sqrt())
}

#[rustfmt::skip]
static SINE_TABLE_A: [u64; 14] = [
    0x0000000000000000,
    0x3fbfeaaf00000000,
    0x3fcfaaeec0000000,
    0x3fd7710240000000,
    0x3fdeaee880000000,
    0x3fe2b91e00000000,
    0x3fe5cffc00000000,
    0x3fe88fb780000000,
    0x3feaed5480000000,
    0x3fecdf6040000000,
    0x3fee5e1500000000,
    0x3fef6379c0000000,
    0x3fefeb7a80000000,
    0x3feff3f800000000,
];
#[rustfmt::skip]
static SINE_TABLE_B: [u64; 14] = [
    0x0000000000000000,
    0xbe317911ca35f965,
    0x3e44f31576ba89df,
    0x3e55764213d22a52,
    0xbe4769f4202f1368,
    0xbe6577bde27e8dc8,
    0x3e66bf8f0d65b2c7,
    0xbe6bf4725e52661e,
    0x3e5e1219dc0831ba,
    0x3e54395b9ba52bde,
    0xbe2eebe73e0d9be0,
    0x3e6619369d5ac9de,
    0x3e6b2c6d8ade6d02,
    0xbe1166cb2886ea80,
];
#[rustfmt::skip]
static COSINE_TABLE_A: [u64; 14] = [
    0x3ff0000000000000,
    0x3fefc01540000000,
    0x3fef015480000000,
    0x3fedc6b800000000,
    0x3fec152800000000,
    0x3fe9f36900000000,
    0x3fe769fec0000000,
    0x3fe4830bc0000000,
    0x3fe14a2800000000,
    0x3fdb986580000000,
    0x3fd42e3dc0000000,
    0x3fc8e6f080000000,
    0x3fb21bd540000000,
    0xbfabbd1b00000000,
];
#[rustfmt::skip]
static COSINE_TABLE_B: [u64; 14] = [
    0x0000000000000000,
    0x3e627d5bd36da3cd,
    0x3e6f7deea174f07a,
    0xbe6466a6edf5a64e,
    0x3e496df53e76deee,
    0xbe626ed07b474808,
    0x3e4954847b9f5d96,
    0x3e67d4ceb377de00,
    0x3e5f6a0d17247090,
    0xbe5c62fa6a64c6fc,
    0x3e588bd951d9589e,
    0xbe34acf053f96efe,
    0x3e2f8bf34e87d450,
    0x3debc96115437580,
];
#[rustfmt::skip]
static TANGENT_TABLE_A: [u64; 14] = [
    0x0000000000000000,
    0x3fc01577c0000000,
    0x3fd05785c0000000,
    0x3fd9312d80000000,
    0x3fe17b4f40000000,
    0x3fe7166680000000,
    0x3fedcfa380000000,
    0x3ff328a380000000,
    0x3ff8eb2440000000,
    0x4000bd9600000000,
    0x4008139940000000,
    0x40142aebc0000000,
    0x402c33ed40000000,
    0xc0326e4d00000000,
];
#[rustfmt::skip]
static TANGENT_TABLE_B: [u64; 14] = [
    0x0000000000000000,
    0xbe40eaee5b1fba5f,
    0xbe5bc3b3aa19c6c0,
    0x3e366fe2bf10b114,
    0x3e6bf3474a431796,
    0x3e53a83ddf05d806,
    0xbe6eef11141f3c06,
    0x3e75115a5dbf6d33,
    0x3e7cbee3a5b8acc8,
    0x3e532451b242ac7c,
    0x3e5f118d40a85840,
    0x3e953f29e3a7b3e9,
    0x3ea0b887775a6a07,
    0xbe9685c459b57c10,
];
#[rustfmt::skip]
static EIGHTHS: [u64; 14] = [
    0x0000000000000000,
    0x3fc0000000000000,
    0x3fd0000000000000,
    0x3fd8000000000000,
    0x3fe0000000000000,
    0x3fe4000000000000,
    0x3fe8000000000000,
    0x3fec000000000000,
    0x3ff0000000000000,
    0x3ff2000000000000,
    0x3ff4000000000000,
    0x3ff6000000000000,
    0x3ff8000000000000,
    0x3ffa000000000000,
];
#[rustfmt::skip]
static LN_HI_PREC_COEF: [[u64; 2]; 6] = [
    [0x3ff0000000000000, 0xbb523b2b485a0b6d],
    [0xbfd0000000000000, 0xbfd0000000000000],
    [0x3fd5555540000000, 0x3e55555197ea2f51],
    [0xbfcfffffc0000000, 0xbe5fc028d6b7ef77],
    [0x3fc99995c0000000, 0x3de5c2091d0952df],
    [0xbfc547a400000000, 0xbe5bf41f86a90827],
];
#[rustfmt::skip]
static LN_MANT: [[u64; 2]; 1024] = [
    [0x0000000000000000, 0x0000000000000000],
    [0x3f4ffc00c0000000, 0xbdc5754ef043fb30],
    [0x3f5ff802c0000000, 0xbdd654ef19875872],
    [0x3f67f70480000000, 0xbdb433e12cbb14c0],
    [0x3f6ff00ac0000000, 0xbded4ef43fb5f794],
    [0x3f73f38a80000000, 0xbdff0f9b76a5b0da],
    [0x3f77ee1200000000, 0xbdf427d16c587a5c],
    [0x3f7be79c80000000, 0xbdeff4e26e0cb280],
    [0x3f7fe02a80000000, 0xbdf4ef987703c896],
    [0x3f81ebde40000000, 0xbe02e6681a0ffd22],
    [0x3f83e72940000000, 0x3e0d25a7d8f80359],
    [0x3f85e1f700000000, 0x3ddf65f2827d6168],
    [0x3f87dc4740000000, 0x3e0f810a76dd2513],
    [0x3f89d61ac0000000, 0xbe023942734e6c28],
    [0x3f8bcf7140000000, 0xbe038bc7b43ed085],
    [0x3f8dc84b00000000, 0x3e09123814a83c74],
    [0x3f8fc0a8c0000000, 0xbdfe07f83860c24c],
    [0x3f90dc4500000000, 0x3e18afcc87a18ffc],
    [0x3f91d7f800000000, 0xbe146114193a83fd],
    [0x3f92d36d00000000, 0xbe104aa83d132fa4],
    [0x3f93cea440000000, 0x3dea352ba779a528],
    [0x3f94c99e00000000, 0x3df24077b31977d7],
    [0x3f95c45a40000000, 0x3e11b8d38893bd25],
    [0x3f96bed940000000, 0x3e01a36fa1df4fac],
    [0x3f97b91b00000000, 0x3dff56c46aa49fd0],
    [0x3f98b31fc0000000, 0xbe13564ff1e2a5b9],
    [0x3f99ace740000000, 0x3e151cc5144d0270],
    [0x3f9aa67200000000, 0x3e1ee835a9d6b8b6],
    [0x3f9b9fc040000000, 0xbe18506e68042b9c],
    [0x3f9c98d180000000, 0x3e0a019027abe9dc],
    [0x3f9d91a680000000, 0xbe13abc33c1d34e6],
    [0x3f9e8a3f00000000, 0xbe1cf32353f47bcb],
    [0x3f9f829b00000000, 0x3e0cf0660099f1fa],
    [0x3fa03d5d80000000, 0x3e079cfbbc4b0768],
    [0x3fa0b94f80000000, 0xbdff34f44c4b7812],
    [0x3fa1352380000000, 0xbe0e9a3835876e43],
    [0x3fa1b0d980000000, 0x3e1247b2ff859460],
    [0x3fa22c71c0000000, 0xbdf8abe2d7e70778],
    [0x3fa2a7ec40000000, 0xbe2deb178ce77a92],
    [0x3fa32348c0000000, 0x3e0c005a5b6e42c8],
    [0x3fa39e87c0000000, 0xbe08050a815bfa94],
    [0x3fa419a900000000, 0x3e12b26d73719f54],
    [0x3fa494acc0000000, 0x3dfa6c88e3c52b80],
    [0x3fa50f9300000000, 0xbe13e0cd2609ead8],
    [0x3fa58a5bc0000000, 0xbe20371b2b739570],
    [0x3fa6050700000000, 0xbde6725748103280],
    [0x3fa67f9500000000, 0xbe1ed684cf830606],
    [0x3fa6fa0580000000, 0x3e23c7b682bc83ad],
    [0x3fa7745900000000, 0xbe139a4607739620],
    [0x3fa7ee8f40000000, 0xbe2a32be842c234a],
    [0x3fa868a840000000, 0xbe1ef812623a134c],
    [0x3fa8e2a440000000, 0xbe2bc5e46114fec0],
    [0x3fa95c8300000000, 0x3e1d91c7d6fad072],
    [0x3fa9d64500000000, 0xbdf002ec36fac65e],
    [0x3faa4fea00000000, 0xbdc70b72c2502040],
    [0x3faac97240000000, 0xbe2e8ee072c10fec],
    [0x3fab42dd80000000, 0xbe1dcd1c827ae5d6],
    [0x3fabbc2c00000000, 0xbdfdd85f461a4500],
    [0x3fac355dc0000000, 0x3e20921f2ccc9ac0],
    [0x3facae7300000000, 0xbe01a8f7d5f9012f],
    [0x3fad276b80000000, 0x3e15b616a423c788],
    [0x3fada04780000000, 0x3e17c724a58f63f0],
    [0x3fae190700000000, 0x3e184ec02b73373c],
    [0x3fae91aa00000000, 0x3e2914f85f57f88c],
    [0x3faf0a30c0000000, 0x3da162a6617cc800],
    [0x3faf829b00000000, 0x3e1cf0660099f1f8],
    [0x3faffae900000000, 0x3e219b9303374264],
    [0x3fb0398d80000000, 0xbe349ddab14dfcc0],
    [0x3fb0759840000000, 0xbe24ce371d9fc96b],
    [0x3fb0b19500000000, 0xbe31f2e99915b097],
    [0x3fb0ed8380000000, 0x3e3b5526fe0b92b7],
    [0x3fb1296440000000, 0x3e100b8ab1122b98],
    [0x3fb1653700000000, 0xbe315c851f179da4],
    [0x3fb1a0fbc0000000, 0xbe3e4075ae019e72],
    [0x3fb1dcb280000000, 0xbe3c24e6bbb0a163],
    [0x3fb2185b40000000, 0xbe122978c627e3cd],
    [0x3fb253f640000000, 0xbe30f5ebe9070496],
    [0x3fb28f8340000000, 0x3e143b6a35d6a845],
    [0x3fb2cb0280000000, 0x3e0faef0f7165338],
    [0x3fb3067400000000, 0xbe2ba6eb361d6231],
    [0x3fb341d780000000, 0x3e361bd1d0929984],
    [0x3fb37d2d80000000, 0xbe23af9c60aa9b84],
    [0x3fb3b87580000000, 0x3e38b1b6edf535aa],
    [0x3fb3f3b000000000, 0x3e105003c48e959c],
    [0x3fb42edcc0000000, 0xbdf59b90fc445620],
    [0x3fb469fbc0000000, 0x3e2d76b6cc789e48],
    [0x3fb4a50d40000000, 0xbe15793effc26114],
    [0x3fb4e01100000000, 0x3e2146b5cb9fbe5a],
    [0x3fb51b0740000000, 0xbdef3cf812db0e40],
    [0x3fb555f000000000, 0xbe3bf4af4b687378],
    [0x3fb590cb00000000, 0xbe007f1ebd8546f0],
    [0x3fb5cb9880000000, 0x3e32ed45ef912994],
    [0x3fb60658c0000000, 0xbe36c8af3c4e2116],
    [0x3fb6410b40000000, 0x3e1bf9eac070c342],
    [0x3fb67bb080000000, 0xbe2b227e08db495f],
    [0x3fb6b64840000000, 0xbe2ca037e61426f0],
    [0x3fb6f0d280000000, 0x3e25cad69737c934],
    [0x3fb72b4f80000000, 0x3e10baa3ecccffd0],
    [0x3fb765bf40000000, 0xbe3c5941ecbc035b],
    [0x3fb7a02180000000, 0xbe309b61edb35e7c],
    [0x3fb7da7680000000, 0xbe3284ed337bbb7f],
    [0x3fb814be40000000, 0xbe3c073fc9b8fc4b],
    [0x3fb84ef880000000, 0x3e38e8282a32db96],
    [0x3fb88925c0000000, 0x3e324fae2afabb02],
    [0x3fb8c345c0000000, 0x3e36319b20f5acb4],
    [0x3fb8fd58c0000000, 0xbe3573deec901bcb],
    [0x3fb9375e40000000, 0x3e35595eddc37380],
    [0x3fb97156c0000000, 0x3e3c8f63e03e6502],
    [0x3fb9ab4240000000, 0x3e1880ceb36fb310],
    [0x3fb9e52080000000, 0x3e37f9c9afd65a96],
    [0x3fba1ef1c0000000, 0x3e38061cd4176dfa],
    [0x3fba58b600000000, 0x3e28564647194744],
    [0x3fba926d40000000, 0xbe16d4aa726bd0b6],
    [0x3fbacc1780000000, 0xbe37bccd53c1d05c],
    [0x3fbb05b480000000, 0x3e3bee43fe45831e],
    [0x3fbb3f44c0000000, 0x3e3b221fd153d8de],
    [0x3fbb78c840000000, 0xbe344f125ef7bc3a],
    [0x3fbbb23e80000000, 0x3e3368e974e7caa6],
    [0x3fbbeba800000000, 0x3e38146764874922],
    [0x3fbc2504c0000000, 0xbde0c51d0765e200],
    [0x3fbc5e5480000000, 0x3e2eb78e862bac30],
    [0x3fbc979780000000, 0x3e2af1d23fded104],
    [0x3fbcd0cdc0000000, 0xbddcfb07ac92bcc0],
    [0x3fbd09f740000000, 0xbe34b3b7db9fffe7],
    [0x3fbd4313c0000000, 0x3e366cb35d5e4376],
    [0x3fbd7c23c0000000, 0x3e1a72cd02364598],
    [0x3fbdb52700000000, 0x3df87d9277856af6],
    [0x3fbdee1d80000000, 0x3e29abdae4bf1ac0],
    [0x3fbe270780000000, 0xbe31d50d1a161578],
    [0x3fbe5fe4c0000000, 0xbe34d8d5fae9f74c],
    [0x3fbe98b540000000, 0x3e22ce28cee91388],
    [0x3fbed17940000000, 0x3e2d06f0175e45b1],
    [0x3fbf0a30c0000000, 0x3db162a6617cca00],
    [0x3fbf42dbc0000000, 0xbe3c5dd312b732e0],
    [0x3fbf7b7a00000000, 0xbdf3c822130fd140],
    [0x3fbfb40bc0000000, 0x3e36ff4e05f65bda],
    [0x3fbfec9140000000, 0xbe2c482a8aaba35c],
    [0x3fc0128500000000, 0x3e34dbf95e79adfc],
    [0x3fc02ebb40000000, 0x3e15f9ea5705a324],
    [0x3fc04aeb40000000, 0x3e227d9afa9065cc],
    [0x3fc0671500000000, 0x3e42ca596e2a18ca],
    [0x3fc08338c0000000, 0xbe4005d7d661a9bf],
    [0x3fc09f5600000000, 0x3e4ee719c33ea3aa],
    [0x3fc0bb6d80000000, 0xbe4db8555cd86198],
    [0x3fc0d77e80000000, 0xbe197b8d34cb4470],
    [0x3fc0f38980000000, 0xbe3d969da359e895],
    [0x3fc10f8e40000000, 0x3e1129cd898f7990],
    [0x3fc12b8d00000000, 0xbe3b5ceb75b5ec3f],
    [0x3fc1478580000000, 0x3e219d0ab1a28818],
    [0x3fc1637800000000, 0xbe23b6f9b8795033],
    [0x3fc17f6440000000, 0x3e48fca610feb426],
    [0x3fc19b4ac0000000, 0xbe4f312ec4124b11],
    [0x3fc1b72ac0000000, 0x3e452f67a0290604],
    [0x3fc1d30500000000, 0xbe2cf2b4da89be88],
    [0x3fc1eed900000000, 0x3e3c5b858558dc26],
    [0x3fc20aa700000000, 0x3e48102f964a1a44],
    [0x3fc2266f00000000, 0x3e490a5acb7d51f0],
    [0x3fc2423100000000, 0x3e43ba50e2ffc388],
    [0x3fc25ded00000000, 0x3e3578d5a3854f18],
    [0x3fc279a300000000, 0x3df569ef432b3240],
    [0x3fc2955300000000, 0xbe2f802b72cfe894],
    [0x3fc2b0fd00000000, 0xbe389cba20d7073a],
    [0x3fc2cca100000000, 0xbe34141b5ef191ae],
    [0x3fc2e83f00000000, 0x3e080d079ea9ebc0],
    [0x3fc303d700000000, 0x3e48e47fd2fe9462],
    [0x3fc31f6940000000, 0xbe04e6699c9b9690],
    [0x3fc33af580000000, 0xbe3511e361335d02],
    [0x3fc3567bc0000000, 0xbddeeb6bcb6b1980],
    [0x3fc371fc40000000, 0xbe4fe1708bc4326a],
    [0x3fc38d7680000000, 0x3e4916430894b3c6],
    [0x3fc3a8eb40000000, 0xbe42ce5c8a091055],
    [0x3fc3c459c0000000, 0x3e4ef76f6e437b5c],
    [0x3fc3dfc2c0000000, 0xbe3e2673ac6acea2],
    [0x3fc3fb25c0000000, 0xbe4a6ad73520ca66],
    [0x3fc41682c0000000, 0xbdf1b08802384100],
    [0x3fc431da00000000, 0x3e00508241bc1b60],
    [0x3fc44d2b80000000, 0xbe433482e1982c26],
    [0x3fc4687700000000, 0x3e250bc4fd740e54],
    [0x3fc483bcc0000000, 0x3e39cdc7ba949c90],
    [0x3fc49efcc0000000, 0x3e28c4f9bc299828],
    [0x3fc4ba3700000000, 0xbe38cb54354bb3f2],
    [0x3fc4d56b40000000, 0x3e4798ec02c00ad6],
    [0x3fc4f09a00000000, 0xbe36bb9e9bf2fafe],
    [0x3fc50bc2c0000000, 0x3e3a5386b695ca36],
    [0x3fc526e600000000, 0xbe4c5e4bc85d1bfe],
    [0x3fc5420340000000, 0xbe2615e97297127c],
    [0x3fc55d1ac0000000, 0x3e44232d6eca6ed3],
    [0x3fc5782cc0000000, 0xbe39edd3a4635178],
    [0x3fc59338c0000000, 0x3e49982085d345bc],
    [0x3fc5ae3f40000000, 0x3e347548f512f809],
    [0x3fc5c94000000000, 0x3e2d65cae4d6e658],
    [0x3fc5e43b00000000, 0x3e435bd5e2a5988a],
    [0x3fc5ff3080000000, 0xbe3eb0d8586f183c],
    [0x3fc61a2040000000, 0xbe4e5f1113117212],
    [0x3fc6350a40000000, 0xbe475558a84fd520],
    [0x3fc64fee80000000, 0x3e304bf896fca296],
    [0x3fc66acd40000000, 0x3e13956a86f6ff1c],
    [0x3fc685a640000000, 0x3e49eefb8355a4fd],
    [0x3fc6a079c0000000, 0x3e40f7aad1fc2247],
    [0x3fc6bb47c0000000, 0xbe4617fcc1365b82],
    [0x3fc6d61000000000, 0xbe48e62de372ab8a],
    [0x3fc6f0d280000000, 0x3e35cad69737c934],
    [0x3fc70b8f80000000, 0x3e47a1aa75327b54],
    [0x3fc7264700000000, 0x3e3f47e88b20f18c],
    [0x3fc740f900000000, 0xbe357f90b66c9904],
    [0x3fc75ba540000000, 0x3e3591dddca58bc4],
    [0x3fc7764c00000000, 0x3e428f2127490346],
    [0x3fc790ed40000000, 0x3e3dc4c666ef539e],
    [0x3fc7ab8900000000, 0x3e1086c848df1b58],
    [0x3fc7c61f40000000, 0xbe4198c91c96d990],
    [0x3fc7e0afc0000000, 0x3e4630c273cf83b3],
    [0x3fc7fb3b00000000, 0xbe2122afb7ea48c0],
    [0x3fc815c0c0000000, 0xbe4ebca815297c91],
    [0x3fc83040c0000000, 0x3e32379e5b71c6e7],
    [0x3fc84abb80000000, 0xbe34f35d8daadf4d],
    [0x3fc86530c0000000, 0xbe4738f33a279a5a],
    [0x3fc87fa080000000, 0xbe4adf36ef6fdff6],
    [0x3fc89a0ac0000000, 0xbe432b18fd01095c],
    [0x3fc8b46f80000000, 0x3e111b12d9f2102e],
    [0x3fc8cecf00000000, 0xbe48b52d2dd05521],
    [0x3fc8e928c0000000, 0x3e4e886d40aa589f],
    [0x3fc9037d80000000, 0xbe45e7fb3c856a0e],
    [0x3fc91dcc80000000, 0x3e386817bc6abde0],
    [0x3fc9381640000000, 0x3e2c565673852b90],
    [0x3fc9525a80000000, 0x3e4cf456b4764130],
    [0x3fc96c9980000000, 0x3e4006a6a235672e],
    [0x3fc986d340000000, 0xbe4d7e7f36254d92],
    [0x3fc9a10740000000, 0x3e46988592a9890c],
    [0x3fc9bb3640000000, 0xbe4182047caa2874],
    [0x3fc9d55fc0000000, 0xbe439d2fa334fe20],
    [0x3fc9ef83c0000000, 0x3e42769a33d21302],
    [0x3fca09a2c0000000, 0xbe4d186776677350],
    [0x3fca23bc00000000, 0x3e4fe2b563193710],
    [0x3fca3dd040000000, 0x3e37270cbed09b34],
    [0x3fca57df40000000, 0xbe47dbb2331b9af2],
    [0x3fca71e8c0000000, 0xbe30840885653778],
    [0x3fca8bed00000000, 0xbe1bbe8739e8c378],
    [0x3fcaa5ec00000000, 0xbe2d5b2541526d2d],
    [0x3fcabfe5c0000000, 0xbe41b9edb47a9caf],
    [0x3fcad9da00000000, 0x3e4f8273bea41c00],
    [0x3fcaf3c940000000, 0x3e3d017fe5b19cc2],
    [0x3fcb0db340000000, 0xbe14ef8047af1c50],
    [0x3fcb279800000000, 0xbe41b9cdf4454836],
    [0x3fcb417780000000, 0xbe4cb455517f54cc],
    [0x3fcb5b5180000000, 0x3e4e8fb5a46e8a00],
    [0x3fcb7526c0000000, 0xbe4dd1b8fc903462],
    [0x3fcb8ef680000000, 0xbe3f7be78acccde8],
    [0x3fcba8c100000000, 0x3e35c8da32e29eee],
    [0x3fcbc28680000000, 0xbe37a4e653ac0c64],
    [0x3fcbdc46c0000000, 0xbe41cbb4ed25fee1],
    [0x3fcbf601c0000000, 0xbe23c6ec7956b840],
    [0x3fcc0fb780000000, 0x3e4ccfd5c800330c],
    [0x3fcc296840000000, 0x3e458c18c0a30848],
    [0x3fcc431400000000, 0xbe48ab16c320c59b],
    [0x3fcc5cba40000000, 0x3e443ae424a77b2d],
    [0x3fcc765b80000000, 0x3e4e4d68d5b59b4b],
    [0x3fcc8ff7c0000000, 0x3e2e6a6886b09768],
    [0x3fcca98ec0000000, 0x3e422f5f4b39f39a],
    [0x3fccc320c0000000, 0x3dc7650240e69800],
    [0x3fccdcad80000000, 0x3e435d1e5f00fcb3],
    [0x3fccf63540000000, 0x3e3c138bb891cd06],
    [0x3fcd0fb800000000, 0xbe3bb5435ea5640d],
    [0x3fcd293580000000, 0x3e0b6b3e6c7f6bc0],
    [0x3fcd42ae00000000, 0xbe03ca2eb801fb08],
    [0x3fcd5c2180000000, 0xbe44b0446ea46ef4],
    [0x3fcd758fc0000000, 0x3e32bdfa97bd5d34],
    [0x3fcd8ef900000000, 0x3e4af31d5df40794],
    [0x3fcda85d80000000, 0xbe4df31aff9acfa4],
    [0x3fcdc1bcc0000000, 0xbe4f5413829f2cea],
    [0x3fcddb16c0000000, 0x3e48ce9c2210d1e0],
    [0x3fcdf46c00000000, 0x3e38e45a5e82b0bc],
    [0x3fce0dbc40000000, 0xbe136aa9b7303eb7],
    [0x3fce270780000000, 0xbe41d50d1a161578],
    [0x3fce404dc0000000, 0xbe4fcb48c3f7704e],
    [0x3fce598ec0000000, 0x3e45a87e2ecb430f],
    [0x3fce72cb00000000, 0x3e407da3ba9199bc],
    [0x3fce8c0240000000, 0x3e42aa5a5fe91fc8],
    [0x3fcea53480000000, 0x3e4e23ac0dc9a63a],
    [0x3fcebe6200000000, 0xbe364509ea299884],
    [0x3fced78a40000000, 0x3e48ca82de36a6f2],
    [0x3fcef0adc0000000, 0x3e37b8b26ca431c0],
    [0x3fcf09cc40000000, 0x3e40036ed04a0c6c],
    [0x3fcf22e600000000, 0xbe48d0efa2a78a0c],
    [0x3fcf3bfa80000000, 0x3e434d6767caa4df],
    [0x3fcf550a40000000, 0x3e464b7b370717dc],
    [0x3fcf6e1540000000, 0xbe3bd59c9a977ac8],
    [0x3fcf871b40000000, 0xbe476aafbafeb529],
    [0x3fcfa01c40000000, 0xbe212a0d0fb8fd78],
    [0x3fcfb91880000000, 0xbe42a1c1d572aaba],
    [0x3fcfd20fc0000000, 0xbdf12047156dcb40],
    [0x3fcfeb0240000000, 0xbe3833f06748e696],
    [0x3fd001f800000000, 0xbe59b7b9d5aa2fce],
    [0x3fd00e6c40000000, 0x3e36b540731a3550],
    [0x3fd01ade40000000, 0xbe3bb184224c3334],
    [0x3fd0274dc0000000, 0x3e16c232f3d139f2],
    [0x3fd033bac0000000, 0x3e5fa740e185e0a4],
    [0x3fd0402580000000, 0x3e54b4d040dae27c],
    [0x3fd04c8e00000000, 0xbe5e7be1fe6ba512],
    [0x3fd058f3c0000000, 0x3e3c0faf1799432e],
    [0x3fd0655740000000, 0x3e3889c2263bf68c],
    [0x3fd071b840000000, 0x3e5fcd590d1d1708],
    [0x3fd07e1700000000, 0x3e54f1c840ce6c33],
    [0x3fd08a7380000000, 0xbe5983a850f8afd0],
    [0x3fd096cd40000000, 0x3e555917e6634814],
    [0x3fd0a32500000000, 0xbe5d8c6f1ca08c08],
    [0x3fd0af7a00000000, 0x3e4d6d84f33e5252],
    [0x3fd0bbccc0000000, 0x3e5b0d24bccb3317],
    [0x3fd0c81d40000000, 0x3e40c150d2185ff2],
    [0x3fd0d46b40000000, 0x3e579ab74b207d91],
    [0x3fd0e0b700000000, 0x3e434869501c53c2],
    [0x3fd0ed0040000000, 0x3e5f657da4715af5],
    [0x3fd0f94740000000, 0x3e59c669ab67a03e],
    [0x3fd1058c00000000, 0xbe3946d4ab9d8182],
    [0x3fd111ce40000000, 0x3daf7266e47b4c00],
    [0x3fd11e0e40000000, 0xbe52526348ff11fa],
    [0x3fd12a4bc0000000, 0x3e2c88d4b15f3d68],
    [0x3fd1368700000000, 0x3e249d4582f6cc58],
    [0x3fd142c000000000, 0xbe5465fb8c679e93],
    [0x3fd14ef680000000, 0xbdfde5e97a65dfb0],
    [0x3fd15b2ac0000000, 0xbe07ae7638ed26c0],
    [0x3fd1675cc0000000, 0xbe545459f1fc6337],
    [0x3fd1738c40000000, 0x3e3699a63916ffcc],
    [0x3fd17fb980000000, 0x3e4c2a12baba2c5d],
    [0x3fd18be480000000, 0x3e3764dc167f6f38],
    [0x3fd1980d40000000, 0xbe522bdc9098b0b8],
    [0x3fd1a43380000000, 0x3e3b99fd0af5f398],
    [0x3fd1b05780000000, 0x3e51f07b48ae6490],
    [0x3fd1bc7940000000, 0x3e4fa3919879c5c0],
    [0x3fd1c898c0000000, 0x3e16999fafbc68c0],
    [0x3fd1d4b600000000, 0xbe58695dbab485ce],
    [0x3fd1e0d0c0000000, 0x3e29b8b5f1e55360],
    [0x3fd1ece940000000, 0x3e5528ae7b184f64],
    [0x3fd1f8ff80000000, 0x3e5e48a2f28d8082],
    [0x3fd2051380000000, 0x3e5f73b05b857858],
    [0x3fd2112540000000, 0x3e5986160b9e2e8a],
    [0x3fd21d34c0000000, 0x3e4ab72daa97d945],
    [0x3fd2294200000000, 0xbe30c21a6976f5e8],
    [0x3fd2354d00000000, 0xbe5a43775e3b417f],
    [0x3fd2415580000000, 0x3e47fa2807ff9915],
    [0x3fd24d5c00000000, 0xbe5095100775e040],
    [0x3fd2596000000000, 0x3e50df7639ef0892],
    [0x3fd2656200000000, 0xbe4d98f3a5bbcc84],
    [0x3fd2716180000000, 0x3e513f853d3c7d85],
    [0x3fd27d5f00000000, 0xbe4c494192374790],
    [0x3fd2895a00000000, 0x3e53de86a35eb493],
    [0x3fd2955300000000, 0xbe3f802b72cfe88c],
    [0x3fd2a14980000000, 0x3e5f762bc9102b82],
    [0x3fd2ad3e00000000, 0x3e456e66e5cb9224],
    [0x3fd2b93040000000, 0xbe351d8b6d896b60],
    [0x3fd2c52040000000, 0xbe4f60fb3deb5cf4],
    [0x3fd2d10e00000000, 0xbe53af7a7ce7823e],
    [0x3fd2dcf980000000, 0xbe50702de96bae3a],
    [0x3fd2e8e2c0000000, 0xbe347b8b3d8f4cdc],
    [0x3fd2f4c9c0000000, 0x3e4e2f4b8f8249ba],
    [0x3fd300aec0000000, 0xbe52f9caf42a5d21],
    [0x3fd30c9140000000, 0x3e557f1ebd6a75ec],
    [0x3fd31871c0000000, 0x3e42a88309f57299],
    [0x3fd3245000000000, 0x3e42ae0e938dcdec],
    [0x3fd3302c00000000, 0x3e565865885f70aa],
    [0x3fd33c0600000000, 0xbe4dae44ae418e04],
    [0x3fd347dd80000000, 0x3e5a987d54d64568],
    [0x3fd353b300000000, 0x3e5376decce50194],
    [0x3fd35f8640000000, 0x3e5c93293e086c58],
    [0x3fd36b5780000000, 0xbe4287ddd2d0f4ac],
    [0x3fd3772680000000, 0xbe5d4027a50dab15],
    [0x3fd382f340000000, 0xbe5e93b1061d21d3],
    [0x3fd38ebdc0000000, 0xbe48e259be9f4660],
    [0x3fd39a8600000000, 0x3e59f4518ee51936],
    [0x3fd3a64c40000000, 0x3e556945e9c72f36],
    [0x3fd3b21080000000, 0xbe594644f07b6605],
    [0x3fd3bdd240000000, 0x3e4d6296d425b478],
    [0x3fd3c99200000000, 0x3e4c364283851548],
    [0x3fd3d54fc0000000, 0xbe5a3e08f078c997],
    [0x3fd3e10b00000000, 0x3e567017cef3c515],
    [0x3fd3ecc480000000, 0xbe5f10a0b014313e],
    [0x3fd3f87b80000000, 0x3e3824ea2028aba6],
    [0x3fd4043080000000, 0x3e3a1a9f8ef43050],
    [0x3fd40fe380000000, 0xbe5ccfc868e80618],
    [0x3fd41b9400000000, 0x3e5cce0bee5bfb06],
    [0x3fd42742c0000000, 0xbe47b04cc393815c],
    [0x3fd432ef40000000, 0xbe55fb17ec4a64c6],
    [0x3fd43e9980000000, 0xbe0a7dc7811b7e60],
    [0x3fd44a41c0000000, 0xbe47387708eb8642],
    [0x3fd455e7c0000000, 0x3e44e4035b18c6cd],
    [0x3fd4618bc0000000, 0x3e20e2f613e85bd8],
    [0x3fd46d2d80000000, 0x3e5c280566c9d7d9],
    [0x3fd478cd40000000, 0x3e5959b3d9137e1a],
    [0x3fd4846b00000000, 0xbe36290b1f5171bc],
    [0x3fd4900680000000, 0x3df0027433001e20],
    [0x3fd49ba000000000, 0xbe5483e894f8e9d3],
    [0x3fd4a73740000000, 0xbe2898334469d7f0],
    [0x3fd4b2cc80000000, 0xbe45554c07e5e3c6],
    [0x3fd4be5f80000000, 0x3e557778a0db4c99],
    [0x3fd4c9f080000000, 0x3e5e152c3c4431e6],
    [0x3fd4d57f80000000, 0x3e4fdfc4fe499ff4],
    [0x3fd4e10c80000000, 0xbe54375f9f5f03a7],
    [0x3fd4ec9740000000, 0xbe4b3ffb2ca15440],
    [0x3fd4f82000000000, 0xbe5b89c2ff9ec86c],
    [0x3fd503a680000000, 0x3e2658e597370e90],
    [0x3fd50f2b00000000, 0x3e4c3c0d04a04625],
    [0x3fd51aad80000000, 0x3e3cb7e0b42724f8],
    [0x3fd5262e00000000, 0xbe51467786e7dc22],
    [0x3fd531ac40000000, 0x3e35fb9df83b7d94],
    [0x3fd53d2880000000, 0x3e4877bddd8d0294],
    [0x3fd548a2c0000000, 0x3e2d6e93167e6304],
    [0x3fd5541b00000000, 0xbe536e40605aab72],
    [0x3fd55f9100000000, 0x3e3e90fb866e21c8],
    [0x3fd56b0500000000, 0x3e55a183248f778a],
    [0x3fd5767700000000, 0x3e57455a6c549ab7],
    [0x3fd581e700000000, 0x3e4a965c19c3c9dc],
    [0x3fd58d5500000000, 0xbe3e47f437067ac4],
    [0x3fd598c0c0000000, 0x3e596879e82da3ca],
    [0x3fd5a42ac0000000, 0xbe4e16603cc75e60],
    [0x3fd5af9280000000, 0xbde990e0197fb000],
    [0x3fd5baf840000000, 0x3e3aa86c6517078d],
    [0x3fd5c65c00000000, 0x3e39166601f9fe82],
    [0x3fd5d1bdc0000000, 0xbe04fec6b5ee4e40],
    [0x3fd5dd1d80000000, 0xbe4acc8c57b0c40a],
    [0x3fd5e87b40000000, 0xbe5f3d6ab60b9c22],
    [0x3fd5f3d6c0000000, 0x3e45144bd7c2470e],
    [0x3fd5ff3080000000, 0xbe4eb0d8586f183a],
    [0x3fd60a8800000000, 0x3e53d1a5ba6e721d],
    [0x3fd615ddc0000000, 0xbe46827d8706adec],
    [0x3fd6213140000000, 0x3e5424eb1f40ed58],
    [0x3fd62c8300000000, 0xbe4a8c70d5a12141],
    [0x3fd637d280000000, 0x3e51329a82284329],
    [0x3fd6432040000000, 0xbe4f777080fec300],
    [0x3fd64e6bc0000000, 0x3e50a3535fd00d3b],
    [0x3fd659b580000000, 0xbe49f83c1afc49ec],
    [0x3fd664fd00000000, 0x3e5819b8c72af233],
    [0x3fd67042c0000000, 0x3e0307c6148dbd80],
    [0x3fd67b8680000000, 0xbe52cd863814e0b2],
    [0x3fd686c800000000, 0x3e5e9b14aec442bc],
    [0x3fd69207c0000000, 0x3e558459d227a431],
    [0x3fd69d4580000000, 0x3e52a0362c6f6fe8],
    [0x3fd6a88140000000, 0x3e56a039074decea],
    [0x3fd6b3bb40000000, 0xbe5dca6bc276a15a],
    [0x3fd6bef300000000, 0xbe43ddc707036b82],
    [0x3fd6ca28c0000000, 0x3e52e34985685c63],
    [0x3fd6d55cc0000000, 0xbe3a8f2646375168],
    [0x3fd6e08ec0000000, 0xbe55d45e1c73ec6a],
    [0x3fd6ebbec0000000, 0xbe59ff1198f59c12],
    [0x3fd6f6ecc0000000, 0xbe5274dd6df4e042],
    [0x3fd70218c0000000, 0x3e178e954a387100],
    [0x3fd70d4300000000, 0xbe5d876dca2a5990],
    [0x3fd7186b00000000, 0x3e513811934e2678],
    [0x3fd7239140000000, 0x3e4cca01c1d52bdc],
    [0x3fd72eb580000000, 0x3e5aac9ee8086846],
    [0x3fd739d800000000, 0xbe42885ff2c63b68],
    [0x3fd744f880000000, 0xbe5cc0e191e9d181],
    [0x3fd7501700000000, 0xbe5f1d459dfa2de4],
    [0x3fd75b3380000000, 0xbe4f5b0b12d68838],
    [0x3fd7664e00000000, 0x3e5239dbcec12546],
    [0x3fd77166c0000000, 0x3e3d10096999f830],
    [0x3fd77c7d80000000, 0x3e501bb9140f9af2],
    [0x3fd7879280000000, 0xbe52948dc16ca131],
    [0x3fd792a540000000, 0x3e5fdd47a27c15da],
    [0x3fd79db680000000, 0xbe57e50534c64586],
    [0x3fd7a8c580000000, 0x3e3b37d5107d6a22],
    [0x3fd7b3d2c0000000, 0xbe2b05b956a16080],
    [0x3fd7bede00000000, 0x3e446f5f7f3c3e1a],
    [0x3fd7c9e780000000, 0xbe4f80e60b5545c8],
    [0x3fd7d4ef00000000, 0xbe50a11392031f1e],
    [0x3fd7dff480000000, 0x3e407b066208cdf3],
    [0x3fd7eaf840000000, 0xbe31f540f26d3168],
    [0x3fd7f5fa00000000, 0x3e43aaf494619656],
    [0x3fd800fa00000000, 0xbe4846d66a180c90],
    [0x3fd80bf800000000, 0xbe370692648c8110],
    [0x3fd816f400000000, 0x3e5da0d495b5b70c],
    [0x3fd821ee40000000, 0x3e5ea9b160aa0528],
    [0x3fd82ce6c0000000, 0xbe200d9319308b30],
    [0x3fd837dd40000000, 0xbe2de27ed22505e8],
    [0x3fd842d1c0000000, 0x3e5a1e8b17493b15],
    [0x3fd84dc480000000, 0x3e58333c0800ca74],
    [0x3fd858b580000000, 0xbe41b4677aa4d533],
    [0x3fd863a480000000, 0xbe40cc1dfff83f58],
    [0x3fd86e9180000000, 0x3e5a330ba009fcd8],
    [0x3fd8797cc0000000, 0x3e5f9429e350a120],
    [0x3fd8846640000000, 0x3e40c00c47f4ef98],
    [0x3fd88f4dc0000000, 0x3e5539078e931a44],
    [0x3fd89a3380000000, 0x3e3b05096ad69c5c],
    [0x3fd8a51740000000, 0x3e5d9a78038325f0],
    [0x3fd8aff940000000, 0x3e5a66177fe8e9f8],
    [0x3fd8bad980000000, 0xbe21d6142a46df70],
    [0x3fd8c5b7c0000000, 0x3e40b169150faa54],
    [0x3fd8d09440000000, 0xbe34fc2a5a26b9ec],
    [0x3fd8db6ec0000000, 0x3e559e272ca784ec],
    [0x3fd8e64780000000, 0x3e59901f7ba32832],
    [0x3fd8f11e80000000, 0x3e3cd98b1df85da0],
    [0x3fd8fbf380000000, 0x3e5f302481d5f8d6],
    [0x3fd906c700000000, 0xbe5de3b8ac619c5a],
    [0x3fd9119840000000, 0x3e50995d53bfb5ca],
    [0x3fd91c6800000000, 0xbe54ba57c278382b],
    [0x3fd92735c0000000, 0xbe4a81ccb226a4dc],
    [0x3fd93201c0000000, 0xbe585cacc95e4e26],
    [0x3fd93ccbc0000000, 0x3e451f4db3fd780e],
    [0x3fd9479400000000, 0x3e5c2116faba4cde],
    [0x3fd9525a80000000, 0x3e5cf456b4764130],
    [0x3fd95d1f40000000, 0x3e4b4b94143f0f02],
    [0x3fd967e240000000, 0xbe512e79e4df09b6],
    [0x3fd972a340000000, 0x3e11351586970280],
    [0x3fd97d6280000000, 0x3e341a8ee5160b74],
    [0x3fd9882000000000, 0xbe32e5643fcd47af],
    [0x3fd992dbc0000000, 0xbe5b91c9615d819c],
    [0x3fd99d9580000000, 0x3e117e08acba9300],
    [0x3fd9a84d80000000, 0x3e51dde4d257555e],
    [0x3fd9b303c0000000, 0x3e575a361f1cbd27],
    [0x3fd9bdb840000000, 0x3e522679dacd548a],
    [0x3fd9c86b00000000, 0x3e26e04314dd0228],
    [0x3fd9d31c00000000, 0xbe55ec12c5491780],
    [0x3fd9ddcb00000000, 0x3e40cdce8337e520],
    [0x3fd9e87840000000, 0x3e5e6d68e33aecaa],
    [0x3fd9f32400000000, 0xbe534067b40d4972],
    [0x3fd9fdcdc0000000, 0xbe481560c93c9d10],
    [0x3fda0875c0000000, 0xbe46b3c5d1e77b85],
    [0x3fda131c00000000, 0xbe5096bca326d0cb],
    [0x3fda1dc080000000, 0xbe5b2a466a9bfb5e],
    [0x3fda286300000000, 0x3e55822ba625a8ec],
    [0x3fda330400000000, 0x3e2027d31b9a54f6],
    [0x3fda3da340000000, 0xbe550bc43dac2e66],
    [0x3fda484080000000, 0x3e50e5bb0a2bfca8],
    [0x3fda52dc40000000, 0xbe47220df37d56b6],
    [0x3fda5d7600000000, 0x3e56252c37141e2a],
    [0x3fda680e40000000, 0xbe42c57df8df1b52],
    [0x3fda72a480000000, 0x3e566bd9ea16a768],
    [0x3fda7d3940000000, 0xbe43b5609d499d38],
    [0x3fda87cc00000000, 0x3e565db1991e22ee],
    [0x3fda925d40000000, 0xbe40ae74ea8b12ac],
    [0x3fda9cec80000000, 0x3e5a9a08498d4851],
    [0x3fdaa77a40000000, 0xbdeda92e656d1100],
    [0x3fdab20640000000, 0xbe58445f6056604a],
    [0x3fdabc9040000000, 0x3e531183b12b3f3c],
    [0x3fdac718c0000000, 0x3e22c587206058f8],
    [0x3fdad19f80000000, 0xbe43b98be35e7fee],
    [0x3fdadc2480000000, 0xbe50fd1700d51b6d],
    [0x3fdae6a7c0000000, 0xbe5276c051b420f0],
    [0x3fdaf12940000000, 0xbe4b710f29dd98f8],
    [0x3fdafba900000000, 0xbe218bcfa3af7ea0],
    [0x3fdb062700000000, 0x3e50af141b564e6e],
    [0x3fdb10a380000000, 0xbe548678f5a0f1b4],
    [0x3fdb1b1e00000000, 0x3e4d7bf8b6d223cc],
    [0x3fdb259700000000, 0xbe33ced4b5fc0536],
    [0x3fdb300e40000000, 0xbe4e17fab97cac50],
    [0x3fdb3a83c0000000, 0xbe4df724c216d68c],
    [0x3fdb44f780000000, 0xbe30cdc275cd0440],
    [0x3fdb4f6980000000, 0x3e51db25847bf25f],
    [0x3fdb59da00000000, 0xbe4882b07b3aa314],
    [0x3fdb6448c0000000, 0xbe5dfab7d1652956],
    [0x3fdb6eb580000000, 0x3e5d3cf35d9d490c],
    [0x3fdb792100000000, 0xbe5a0caf6cbd7efe],
    [0x3fdb838a80000000, 0xbe2a51f0810cafe8],
    [0x3fdb8df280000000, 0xbe5dee9b62c8794c],
    [0x3fdb985880000000, 0x3e569310fb598fb1],
    [0x3fdba2bd00000000, 0x3e5ac75acd43adea],
    [0x3fdbad2000000000, 0xbe50c56e9a8d8c2b],
    [0x3fdbb78100000000, 0x3e5478bfd245b8e0],
    [0x3fdbc1e080000000, 0x3e461b5a1484f43c],
    [0x3fdbcc3e40000000, 0x3e5384dc1830e340],
    [0x3fdbd69a80000000, 0xbe519673b8e95d58],
    [0x3fdbe0f4c0000000, 0x3e5c46b94e0dab2a],
    [0x3fdbeb4d80000000, 0x3e5da71b7bf7861d],
    [0x3fdbf5a4c0000000, 0xbe49d5b039e5502a],
    [0x3fdbfffa00000000, 0x3e5d1b10844cb98b],
    [0x3fdc0a4dc0000000, 0x3e5c42c47ac188ac],
    [0x3fdc14a000000000, 0xbe4dd41fb2a65e5b],
    [0x3fdc1ef040000000, 0x3e5c1dfd5888eeb2],
    [0x3fdc293f00000000, 0x3e5de4137beb0fd2],
    [0x3fdc338c40000000, 0xbe421dd07b7ba090],
    [0x3fdc3dd7c0000000, 0xbe583252b28c4c40],
    [0x3fdc482180000000, 0xbe4dfb954b547188],
    [0x3fdc526980000000, 0x3e5316cf6b079856],
    [0x3fdc5cb000000000, 0x3e4d26b2e24f2e1e],
    [0x3fdc66f500000000, 0xbe5c009008182947],
    [0x3fdc713800000000, 0x3e53e26e4e00b2ac],
    [0x3fdc7b7980000000, 0x3e5ec36eaf81d6b0],
    [0x3fdc85b980000000, 0x3e34a529be072cf0],
    [0x3fdc8ff7c0000000, 0x3e3e6a6886b0975c],
    [0x3fdc9a3480000000, 0xbe59624a3c51825a],
    [0x3fdca46f80000000, 0xbe5d474b197bbc22],
    [0x3fdcaea8c0000000, 0xbe2c748942f300c0],
    [0x3fdcb8e080000000, 0xbe47650a6c05221e],
    [0x3fdcc31680000000, 0x3e45a45ffdf50c58],
    [0x3fdccd4b00000000, 0x3e0094bde60ae6a0],
    [0x3fdcd77dc0000000, 0x3e55e9b4731315a8],
    [0x3fdce1af00000000, 0x3e470be7d6f6fa56],
    [0x3fdcebdec0000000, 0xbe5e224295f1a95b],
    [0x3fdcf60c80000000, 0x3e59752ad983db73],
    [0x3fdd003900000000, 0xbe4a5fd1b461d54c],
    [0x3fdd0a63c0000000, 0xbe518de19bdaa664],
    [0x3fdd148cc0000000, 0x3e49bd3bf5cbfb56],
    [0x3fdd1eb440000000, 0x3e4d316992c4c72a],
    [0x3fdd28da40000000, 0xbe47b97960f30710],
    [0x3fdd32fe80000000, 0xbe1ff142a9e21360],
    [0x3fdd3d2140000000, 0xbe534bed6d6e7182],
    [0x3fdd474240000000, 0x3e07dbbab15292e0],
    [0x3fdd5161c0000000, 0xbe35732754e9f080],
    [0x3fdd5b7f80000000, 0x3e5ae2c683cb0830],
    [0x3fdd659c00000000, 0xbe5e0103a1de625e],
    [0x3fdd6fb680000000, 0x3e507907ea1c8cea],
    [0x3fdd79cfc0000000, 0xbe592e23ce73b61b],
    [0x3fdd83e740000000, 0xbe5a75d0c1afaea4],
    [0x3fdd8dfd00000000, 0x3e4a44f48195459f],
    [0x3fdd981140000000, 0x3e5e1af9b634f306],
    [0x3fdda22400000000, 0x3e58f3ad8ed7b3f3],
    [0x3fddac3540000000, 0xbe1d3a6abdcf1980],
    [0x3fddb644c0000000, 0x3e4c89089d53d403],
    [0x3fddc052c0000000, 0x3e4376f357fe3eb8],
    [0x3fddca5f40000000, 0xbe4ddf6eb7161e35],
    [0x3fddd46a00000000, 0x3e33071282fb9894],
    [0x3fddde7340000000, 0x3e353a157dd894f0],
    [0x3fdde87b00000000, 0xbe499631caff328b],
    [0x3fddf28100000000, 0x3e4de61558b33db0],
    [0x3fddfc8580000000, 0x3e5906d5b540784e],
    [0x3fde068880000000, 0x3e51edf2c045bb6f],
    [0x3fde108a00000000, 0xbe3768ba5f050a7f],
    [0x3fde1a89c0000000, 0x3e522ba67c9af246],
    [0x3fde248800000000, 0x3e5a7c6c261cbd91],
    [0x3fde2e84c0000000, 0x3e5394e20b759b3c],
    [0x3fde388000000000, 0xbe20739451d217f8],
    [0x3fde427980000000, 0x3e5a0eb974ff35ea],
    [0x3fde4c71c0000000, 0xbe579788fbd330dd],
    [0x3fde566840000000, 0xbe568562d2635fbe],
    [0x3fde605d00000000, 0x3e5dc0c92eee857a],
    [0x3fde6a5080000000, 0x3e36d97a741a38d0],
    [0x3fde744280000000, 0xbe5e2978781c825c],
    [0x3fde7e32c0000000, 0xbe4ac796c34d102c],
    [0x3fde882180000000, 0xbe3df78ab93c2a68],
    [0x3fde920ec0000000, 0xbe47fa800dabd3c8],
    [0x3fde9bfa80000000, 0xbe5a679e0a9b9142],
    [0x3fdea5e480000000, 0x3e4b7a1fa756d24e],
    [0x3fdeafcd40000000, 0xbe5315628ed8ae71],
    [0x3fdeb9b440000000, 0x3e2cd530ef994fa0],
    [0x3fdec399c0000000, 0x3e52468cc0175cee],
    [0x3fdecd7dc0000000, 0x3e59677bf6696913],
    [0x3fded76040000000, 0x3e5976663b509dfe],
    [0x3fdee14140000000, 0x3e52ec06077003f8],
    [0x3fdeeb20c0000000, 0x3e390377d0d61b8a],
    [0x3fdef4fec0000000, 0xbe48258e43630508],
    [0x3fdefedb00000000, 0x3e5c692a0801a534],
    [0x3fdf08b600000000, 0x3de6469db755ba80],
    [0x3fdf128f40000000, 0x3e5faf06ecb35c80],
    [0x3fdf1c6740000000, 0xbe325fee9025be0c],
    [0x3fdf263d80000000, 0x3e53cebbb758ee84],
    [0x3fdf301280000000, 0xbe56a5d3c3346198],
    [0x3fdf39e5c0000000, 0xbe2bf70d2197fc80],
    [0x3fdf43b780000000, 0x3e4b7447ae170780],
    [0x3fdf4d87c0000000, 0x3e5d7b97e5eeb28d],
    [0x3fdf5756c0000000, 0xbe53c460a9f973a8],
    [0x3fdf612400000000, 0xbe363f5d4e7aea58],
    [0x3fdf6aefc0000000, 0x3e411dd88d8f06ea],
    [0x3fdf74ba00000000, 0x3e570d6c7cdf2e48],
    [0x3fdf7e8300000000, 0xbe599f0a3ecb138a],
    [0x3fdf884a40000000, 0xbe4202c27ba73a8e],
    [0x3fdf921000000000, 0x3e42b69115ae9ac4],
    [0x3fdf9bd440000000, 0x3e5deb9ea3b38ad5],
    [0x3fdfa59740000000, 0xbe45b7d1995fa3f9],
    [0x3fdfaf5880000000, 0x3e4ef1e63db35f64],
    [0x3fdfb91880000000, 0xbe52a1c1d572aab8],
    [0x3fdfc2d6c0000000, 0x3e4e8f9e3427c794],
    [0x3fdfcc93c0000000, 0xbe44ad705e789494],
    [0x3fdfd64f40000000, 0xbe5f09ea8e35b616],
    [0x3fdfe00900000000, 0x3e51a170803d6232],
    [0x3fdfe9c180000000, 0x3e403cb9fe278f5e],
    [0x3fdff37880000000, 0x3e337eaa44172cc8],
    [0x3fdffd2e00000000, 0x3e40afe930ab2f9c],
    [0x3fe0037100000000, 0x3e52f9a2567f6d73],
    [0x3fe0084a40000000, 0x3e629b7385e8ecbe],
    [0x3fe00d22c0000000, 0x3e6fc0e10b770197],
    [0x3fe011fac0000000, 0xbe5db400ebcfde46],
    [0x3fe016d1c0000000, 0x3e4c0e6bfa0062ac],
    [0x3fe01ba840000000, 0xbe6e6d9a5c7a8169],
    [0x3fe0207dc0000000, 0x3e20af801b1c9b60],
    [0x3fe02552c0000000, 0xbe6a5a2f01396396],
    [0x3fe02a26c0000000, 0x3e5f36abda6df12b],
    [0x3fe02efa40000000, 0xbe1b85ac8d64e420],
    [0x3fe033cd00000000, 0xbe57110d3819ad2b],
    [0x3fe0389f00000000, 0xbe60319cc4c3d548],
    [0x3fe03d7040000000, 0xbe5d3ee07f076df4],
    [0x3fe04240c0000000, 0xbe4a686acf7a3588],
    [0x3fe0471080000000, 0x3e502c61fe5def04],
    [0x3fe04bdf80000000, 0x3e6da926d265fcc2],
    [0x3fe050ae00000000, 0xbe46a6796a75c904],
    [0x3fe0557b80000000, 0x3e6e55634dad55ba],
    [0x3fe05a4880000000, 0x3e53bb294b841754],
    [0x3fe05f14c0000000, 0xbe36cdd31fab2910],
    [0x3fe063e040000000, 0xbe4e654af8c3b796],
    [0x3fe068ab00000000, 0xbe40a85897967104],
    [0x3fe06d7500000000, 0x3e4eac03a250025a],
    [0x3fe0723e40000000, 0x3e6c1cdf404e5796],
    [0x3fe0770700000000, 0xbe4a769bb5d6ee58],
    [0x3fe07bcec0000000, 0x3e6fb229fd6f76be],
    [0x3fe0809600000000, 0x3e5e85ea3573b810],
    [0x3fe0855c80000000, 0x3e50968a1cb82c14],
    [0x3fe08a2240000000, 0x3e5603182a2e0e60],
    [0x3fe08ee740000000, 0x3e679c2413067f52],
    [0x3fe093abc0000000, 0xbe61aeb15ddcee18],
    [0x3fe0986f40000000, 0x3e5eae6a41723fb6],
    [0x3fe09d3240000000, 0xbe44705d00ab8718],
    [0x3fe0a1f480000000, 0xbe5da53adcb5b894],
    [0x3fe0a6b600000000, 0xbe5b2cf8d0607383],
    [0x3fe0ab76c0000000, 0xbe231eb2e3fad928],
    [0x3fe0b036c0000000, 0x3e6690c74ae8661d],
    [0x3fe0b4f640000000, 0xbe48632f5c242130],
    [0x3fe0b9b500000000, 0xbe66f92345fb2c7e],
    [0x3fe0be7300000000, 0xbe6bdad57d496768],
    [0x3fe0c33040000000, 0xbe648890b3abc1a8],
    [0x3fe0c7ecc0000000, 0xbe19a359a53a7abe],
    [0x3fe0cca880000000, 0x3e6f8caeb2e7ec80],
    [0x3fe0d163c0000000, 0x3e5973ad6fc108cb],
    [0x3fe0d61e40000000, 0x3e4bbd42a6c1e34c],
    [0x3fe0dad800000000, 0x3e5cc3f0f58b15c0],
    [0x3fe0df9140000000, 0xbe6cb96d4a1bcc15],
    [0x3fe0e44980000000, 0x3e4747322fdbab94],
    [0x3fe0e90140000000, 0xbe538f883c5988b4],
    [0x3fe0edb840000000, 0xbe56a34144168d54],
    [0x3fe0f26e80000000, 0x3e2689e762dd5020],
    [0x3fe0f72400000000, 0x3e6c9b497d4ea111],
    [0x3fe0fbd900000000, 0x3e49ea66c3062a6c],
    [0x3fe1008d40000000, 0xbe18a9d73c3dcde0],
    [0x3fe10540c0000000, 0x3e4c3e6de2c6217c],
    [0x3fe109f380000000, 0x3e6e2d4c96fde3ed],
    [0x3fe10ea5c0000000, 0x3e431e8840ba92aa],
    [0x3fe1135740000000, 0xbe43b675f9f07a1c],
    [0x3fe1180800000000, 0x3e2413dc3b37cfe0],
    [0x3fe11cb800000000, 0x3e6787ccf8408e1f],
    [0x3fe1216780000000, 0xbe2e66d6936fbd00],
    [0x3fe1261640000000, 0xbe55ac0b0b6ce100],
    [0x3fe12ac440000000, 0xbe389d308d414b10],
    [0x3fe12f7180000000, 0x3e6593efbc530124],
    [0x3fe1341e40000000, 0xbe158378eeabfb00],
    [0x3fe138ca40000000, 0xbe4682dac2900bb2],
    [0x3fe13d7580000000, 0x3e4ba46a49175c4f],
    [0x3fe1422040000000, 0xbe6adbc2bb0d6870],
    [0x3fe146ca00000000, 0x3e6543806a390610],
    [0x3fe14b7340000000, 0x3e67799cd11383c9],
    [0x3fe1501c00000000, 0xbe6406cade7228ea],
    [0x3fe154c3c0000000, 0x3e62f4d5e9a98f34],
    [0x3fe1596b00000000, 0x3e593deb547210a0],
    [0x3fe15e1180000000, 0x3e6929f4e3b11838],
    [0x3fe162b780000000, 0xbe4cdf87743a5128],
    [0x3fe1675cc0000000, 0xbe645459f1fc6338],
    [0x3fe16c0140000000, 0xbe5bf2a91e5dec9a],
    [0x3fe170a500000000, 0x3e58166e939eea20],
    [0x3fe1754840000000, 0xbe48530ca56e24d4],
    [0x3fe179eac0000000, 0xbe409d997d00e7c0],
    [0x3fe17e8c80000000, 0x3e62051589ec6414],
    [0x3fe1832dc0000000, 0xbe3aece9393a81e8],
    [0x3fe187ce40000000, 0xbe407786e9aa5800],
    [0x3fe18c6e00000000, 0x3e5feb9e0c176512],
    [0x3fe1910d40000000, 0xbe4bc4649657eb58],
    [0x3fe195abc0000000, 0xbe51427bccef81c4],
    [0x3fe19a4980000000, 0x3e562d4f7a8568ce],
    [0x3fe19ee6c0000000, 0xbe57306d22674680],
    [0x3fe1a38340000000, 0xbe58f978da0fc026],
    [0x3fe1a81f00000000, 0x3e51343aacf2924d],
    [0x3fe1acba40000000, 0xbe58456e8bc9ade0],
    [0x3fe1b154c0000000, 0xbe5504bac22023d6],
    [0x3fe1b5ee80000000, 0x3e5b57e5a4ed751a],
    [0x3fe1ba87c0000000, 0xbe3b389f2a37fd0a],
    [0x3fe1bf2040000000, 0x3e33a96212f94c18],
    [0x3fe1c3b800000000, 0x3e6f713c24bc94f9],
    [0x3fe1c84f40000000, 0x3e63bd8fb9d2efa7],
    [0x3fe1cce5c0000000, 0x3e6f8a86224cf085],
    [0x3fe1d17bc0000000, 0x3e38434d9b83fc50],
    [0x3fe1d61100000000, 0xbe2988ffcf7b1530],
    [0x3fe1daa580000000, 0x3e61d7b9f344ec61],
    [0x3fe1df3980000000, 0xbe33b5620b554978],
    [0x3fe1e3ccc0000000, 0x3e2ac35037717320],
    [0x3fe1e85f40000000, 0x3e6e7040d03dec59],
    [0x3fe1ecf140000000, 0x3e640545aed30bf5],
    [0x3fe1f182c0000000, 0xbe6d65024f93fe8e],
    [0x3fe1f61340000000, 0x3e54c21cf27263c0],
    [0x3fe1faa340000000, 0x3e570e12981817b8],
    [0x3fe1ff32c0000000, 0xbe69c394ea0b153b],
    [0x3fe203c140000000, 0x3e6ab09c79d8b41c],
    [0x3fe2084f80000000, 0xbe66ed0e2401c99e],
    [0x3fe20cdcc0000000, 0x3e6192ab6d93503e],
    [0x3fe2116980000000, 0x3e645ef53829e71c],
    [0x3fe215f5c0000000, 0xbe5cb231ada69794],
    [0x3fe21a8140000000, 0xbe66667ae2b41127],
    [0x3fe21f0c00000000, 0xbe3cd208a0f12786],
    [0x3fe2239640000000, 0xbe65c58f8db0bc98],
    [0x3fe2281fc0000000, 0xbe59733cc1603671],
    [0x3fe22ca880000000, 0x3e67b8467e46d334],
    [0x3fe23130c0000000, 0x3e67bebf4282de36],
    [0x3fe235b880000000, 0xbe58ef4f5b0a0833],
    [0x3fe23a3f80000000, 0xbe64bc75acfd3eda],
    [0x3fe23ec5c0000000, 0xbe1c28b9b1224300],
    [0x3fe2434b80000000, 0xbe60b7c6cc1debba],
    [0x3fe247d080000000, 0xbe4046ec179a7c44],
    [0x3fe24c5500000000, 0xbe6ac0f86d2e7448],
    [0x3fe250d8c0000000, 0xbe6497687e540906],
    [0x3fe2555bc0000000, 0x3e5d31ef96780876],
    [0x3fe259de40000000, 0x3e5dfc300009497c],
    [0x3fe25e6040000000, 0xbe633a2bb891d34a],
    [0x3fe262e180000000, 0xbe67e20a954b909c],
    [0x3fe2676200000000, 0x3e23430dfcd2ad60],
    [0x3fe26be200000000, 0xbe4f275caee4d958],
    [0x3fe2706140000000, 0x3e5aa2d8703f02c6],
    [0x3fe274e000000000, 0x3e166ed9129a703b],
    [0x3fe2795e00000000, 0x3e6289b11aeb783e],
    [0x3fe27ddb80000000, 0x3e380b85397dc645],
    [0x3fe2825840000000, 0x3e6247f7d2ee3fb8],
    [0x3fe286d480000000, 0x3e114ebf7cacbfe0],
    [0x3fe28b5000000000, 0x3e5bec0f055e04fc],
    [0x3fe28fcb00000000, 0xbe452128aa74f4c8],
    [0x3fe2944540000000, 0x3e4bf1dde8c44d00],
    [0x3fe298bf00000000, 0xbe5a1d9fcc12c710],
    [0x3fe29d3800000000, 0xbe23d4f7537a3298],
    [0x3fe2a1b080000000, 0xbe65624925879c28],
    [0x3fe2a62840000000, 0xbe52a2234dd994cd],
    [0x3fe2aa9f80000000, 0xbe6cdd105caa2352],
    [0x3fe2af1600000000, 0xbe5fb37ea5c69f38],
    [0x3fe2b38bc0000000, 0x3e6de554cbcad58e],
    [0x3fe2b80140000000, 0xbe6373733f55801a],
    [0x3fe2bc75c0000000, 0x3e6c4836c345fa1c],
    [0x3fe2c0ea00000000, 0xbe62bb7174468564],
    [0x3fe2c55d40000000, 0x3e6fadbd5cfdcafe],
    [0x3fe2c9d040000000, 0xbe58a04be1a86fb8],
    [0x3fe2ce4280000000, 0xbe668916e6473ee0],
    [0x3fe2d2b400000000, 0x3e22edc9dabba750],
    [0x3fe2d72500000000, 0xbe43f1ae40d18f9c],
    [0x3fe2db9540000000, 0x3e6720de22c64fbb],
    [0x3fe2e00500000000, 0x3e65b2751150d563],
    [0x3fe2e47440000000, 0xbe5237fb2f7f57bc],
    [0x3fe2e8e2c0000000, 0xbe447b8b3d8f4cd8],
    [0x3fe2ed50c0000000, 0xbe6e2ab5a656eb9e],
    [0x3fe2f1be00000000, 0xbe6414033d12c3cd],
    [0x3fe2f62a80000000, 0x3e695095462d8d0e],
    [0x3fe2fa96c0000000, 0xbe65d19d3e9c0446],
    [0x3fe2ff0200000000, 0x3e6eb0a220aefc24],
    [0x3fe3036d00000000, 0xbe51fb032f8bce06],
    [0x3fe307d740000000, 0xbe5961de83c094dc],
    [0x3fe30c40c0000000, 0x3e63c1607443ff5c],
    [0x3fe310a9c0000000, 0x3e68846312291522],
    [0x3fe3151240000000, 0x3e2c2fc5ebc69050],
    [0x3fe3197a00000000, 0x3e5f4ffcd43ac694],
    [0x3fe31de140000000, 0x3e32f14cbaea5aac],
    [0x3fe32247c0000000, 0x3e6a102ca6387c70],
    [0x3fe326adc0000000, 0x3e66e8a4bcc8ebc8],
    [0x3fe32b1340000000, 0xbe4bb78a3b37eaa8],
    [0x3fe32f7800000000, 0x3e16e21a68bc9fa0],
    [0x3fe333dc40000000, 0xbe61fe1889dc0e92],
    [0x3fe3383fc0000000, 0x3e21cf879d0de4d8],
    [0x3fe33ca2c0000000, 0xbe4735d9acdf9458],
    [0x3fe3410500000000, 0x3e696ccf1d86b302],
    [0x3fe34566c0000000, 0x3e6ef5ec140fa700],
    [0x3fe349c800000000, 0x3e55efe456d381da],
    [0x3fe34e2880000000, 0x3e6d9ce1d316eb92],
    [0x3fe3528880000000, 0x3e670eaa40851113],
    [0x3fe356e800000000, 0xbe5111ad0f0ef38c],
    [0x3fe35b46c0000000, 0xbe1ffa74f1ff03c0],
    [0x3fe35fa500000000, 0xbe622c915ffdb059],
    [0x3fe3640280000000, 0x3e406a6be8d514c0],
    [0x3fe3685f80000000, 0x3e2ff4b06ced12a0],
    [0x3fe36cbc00000000, 0xbe6854f89c60a89b],
    [0x3fe37117c0000000, 0xbe557170947445fa],
    [0x3fe3757300000000, 0xbe65028d4d121ffa],
    [0x3fe379cd80000000, 0x3e51edbc550dcade],
    [0x3fe37e2780000000, 0x3e5eb99e1deb85ba],
    [0x3fe3828100000000, 0xbe2a786814056f60],
    [0x3fe386d9c0000000, 0x3e661300cdaef40c],
    [0x3fe38b3200000000, 0x3e66b5778df1c679],
    [0x3fe38f89c0000000, 0x3e0a37e64c9d2294],
    [0x3fe393e0c0000000, 0x3e63562a19a9c442],
    [0x3fe3983740000000, 0x3e5f4c6f26560f00],
    [0x3fe39c8d40000000, 0xbe54fc5027abd5ab],
    [0x3fe3a0e280000000, 0x3e444729daec992e],
    [0x3fe3a53740000000, 0xbe28142067363df0],
    [0x3fe3a98b80000000, 0xbe6e0eaf47254be8],
    [0x3fe3addf00000000, 0xbe606def80a2d16c],
    [0x3fe3b23200000000, 0xbe68768558d112a0],
    [0x3fe3b68440000000, 0x3e53fff8455f1dbd],
    [0x3fe3bad600000000, 0x3e671df1855f0a97],
    [0x3fe3bf2740000000, 0x3e5e174e1966a83e],
    [0x3fe3c37800000000, 0xbe5c1d4eae2fed8e],
    [0x3fe3c7c800000000, 0xbdd19bf46fa03480],
    [0x3fe3cc1780000000, 0xbe4aeac5b237db22],
    [0x3fe3d06640000000, 0x3e6e03b98f14fdc6],
    [0x3fe3d4b4c0000000, 0xbe61a591f456578f],
    [0x3fe3d90280000000, 0xbe658ea9055bfbd9],
    [0x3fe3dd4f80000000, 0x3e62704f349fc188],
    [0x3fe3e19c40000000, 0xbe6980de7df936e9],
    [0x3fe3e5e840000000, 0xbe693a775c4e118a],
    [0x3fe3ea3380000000, 0x3e636b2f5b83d9bb],
    [0x3fe3ee7e80000000, 0xbe63684f8b01d4de],
    [0x3fe3f2c8c0000000, 0xbe5b1ad2645e43e6],
    [0x3fe3f71280000000, 0xbe6adca2a19c103d],
    [0x3fe3fb5b80000000, 0x3e4345bd096d3a72],
    [0x3fe3ffa400000000, 0x3e61a427c115163e],
    [0x3fe403ec00000000, 0x3e5785a4ac443ab0],
    [0x3fe4083380000000, 0xbe5956aa8c8e4ccc],
    [0x3fe40c7a40000000, 0x3e5101b9d2453c90],
    [0x3fe410c080000000, 0x3e56dd0a5e467424],
    [0x3fe4150640000000, 0xbe3dda7e9d283ad4],
    [0x3fe4194b40000000, 0x3e6b2a5adfa30c5c],
    [0x3fe41d9000000000, 0xbe67b98d519b9b44],
    [0x3fe421d400000000, 0xbe6c402a931709fe],
    [0x3fe4261740000000, 0x3e5b7aa2cd4eecaa],
    [0x3fe42a5a40000000, 0xbe699a5a99766b12],
    [0x3fe42e9c80000000, 0xbe62207f40d35046],
    [0x3fe432de40000000, 0xbe6bae7cace2f43a],
    [0x3fe4371f40000000, 0x3e53c47b3a1729ce],
    [0x3fe43b5fc0000000, 0x3e6eb830b7fe28dc],
    [0x3fe43fa000000000, 0xbe6d063198858d84],
    [0x3fe443df40000000, 0x3e66cd791fab58f5],
    [0x3fe4481e40000000, 0xbe4699efc9fbe0d8],
    [0x3fe44c5c80000000, 0x3e5b88663d9726fe],
    [0x3fe4509a40000000, 0x3e6133bb0a503f88],
    [0x3fe454d780000000, 0x3e4339055f65a660],
    [0x3fe4591440000000, 0xbe67462397bee17e],
    [0x3fe45d5040000000, 0xbe371b64144d16a8],
    [0x3fe4618bc0000000, 0x3e30e2f613e85bd8],
    [0x3fe465c6c0000000, 0xbe5041b1592ebcb4],
    [0x3fe46a0100000000, 0x3e6e8ac74551900c],
    [0x3fe46e3b00000000, 0xbe5375eca37538a4],
    [0x3fe4727440000000, 0xbe198aaa59ca4c40],
    [0x3fe476ad00000000, 0xbe4a0ea238d93a90],
    [0x3fe47ae540000000, 0xbe6abb51bb99219c],
    [0x3fe47f1cc0000000, 0x3e3591f46f20c700],
    [0x3fe48353c0000000, 0x3e61ea88df73d5e9],
    [0x3fe4878a40000000, 0x3e6312fefa710f04],
    [0x3fe48bc040000000, 0x3e49440f60ac4530],
    [0x3fe48ff5c0000000, 0xbe6436144e53d3f6],
    [0x3fe4942a80000000, 0x3e3d17e03bda18a6],
    [0x3fe4985ec0000000, 0x3e5c02d752b09168],
    [0x3fe49c9280000000, 0x3e5614c57be74670],
    [0x3fe4a0c5c0000000, 0xbe447414d57df6e0],
    [0x3fe4a4f840000000, 0x3e6db03ebb0227bf],
    [0x3fe4a92a80000000, 0xbe58d1aa2e97af92],
    [0x3fe4ad5c00000000, 0xbe3a1a96e892c6f2],
    [0x3fe4b18d00000000, 0xbe4ae96d2781b65c],
    [0x3fe4b5bd80000000, 0xbe66a91d8c721e80],
    [0x3fe4b9ed40000000, 0x3e5a2a5171c7b794],
    [0x3fe4be1cc0000000, 0xbe6b5ad4883c95a2],
    [0x3fe4c24b80000000, 0xbe5fa8df85610e79],
    [0x3fe4c679c0000000, 0xbe603311c64e9714],
    [0x3fe4caa780000000, 0xbe6c5231b8666220],
    [0x3fe4ced480000000, 0x3e57e555e776427d],
    [0x3fe4d30140000000, 0xbe67400f7bbd98be],
    [0x3fe4d72d40000000, 0xbe47180bfee32b29],
    [0x3fe4db58c0000000, 0x3e10a406e735bf80],
    [0x3fe4df83c0000000, 0xbe40e990ded5dd3d],
    [0x3fe4e3ae40000000, 0xbe63e05cd6318b16],
    [0x3fe4e7d800000000, 0x3e61b75bb09cb098],
    [0x3fe4ec0180000000, 0xbe634f2368972ce6],
    [0x3fe4f02a40000000, 0xbe367e87cf417588],
    [0x3fe4f45280000000, 0x3e3aca70c881f3a0],
    [0x3fe4f87a40000000, 0xbe15fb22ef465400],
    [0x3fe4fca180000000, 0xbe5d8ecee07fd44e],
    [0x3fe500c800000000, 0x3e693674345ca1f0],
    [0x3fe504ee40000000, 0xbe512505e4e772f8],
    [0x3fe50913c0000000, 0x3e5802d0d6979676],
    [0x3fe50d38c0000000, 0x3e6715ddbd97e516],
    [0x3fe5115d40000000, 0x3e68ce769b9c878b],
    [0x3fe5158140000000, 0x3e614ebecadf8f96],
    [0x3fe519a4c0000000, 0x3e174688ccd99080],
    [0x3fe51dc7c0000000, 0xbe68cbb8ba26960e],
    [0x3fe521ea00000000, 0x3e4381691272fa78],
    [0x3fe5260bc0000000, 0x3e69e1c208ec83ad],
    [0x3fe52a2d40000000, 0xbe69a43a55118838],
    [0x3fe52e4e00000000, 0xbe658e61643e8e22],
    [0x3fe5326e40000000, 0xbe69b98753781936],
    [0x3fe5368dc0000000, 0x3e69fd72156de59a],
    [0x3fe53aad00000000, 0x3e46e66df2aa374c],
    [0x3fe53ecbc0000000, 0xbe6662080282fca0],
    [0x3fe542e9c0000000, 0x3e4735f4bc938d5a],
    [0x3fe5470740000000, 0x3e6a6b132bd63d6d],
    [0x3fe54b2480000000, 0xbe68666b6856eabe],
    [0x3fe54f4100000000, 0xbe628431587b90e3],
    [0x3fe5535d00000000, 0xbe63cb7ed7fb148c],
    [0x3fe5577880000000, 0xbe6c19a17ba10b2e],
    [0x3fe55b9340000000, 0x3e64b40bcd3c8330],
    [0x3fe55fadc0000000, 0xbe23fdf4069876a0],
    [0x3fe563c7c0000000, 0xbe6dd2d8347665f2],
    [0x3fe567e100000000, 0xbe1c4c3c9b1af180],
    [0x3fe56bf9c0000000, 0x3e65b3f399411c64],
    [0x3fe5701240000000, 0xbe69ed7507cdf73e],
    [0x3fe5742a00000000, 0xbe5f488c8ebeb1c6],
    [0x3fe5784140000000, 0xbe569c6f9f81dd94],
    [0x3fe57c5800000000, 0xbe59921cdef4a9d0],
    [0x3fe5806e40000000, 0xbe63f29c12963f12],
    [0x3fe58483c0000000, 0x3e6f5740440fdc0f],
    [0x3fe5889900000000, 0x3e5a6d3502f3a490],
    [0x3fe58cadc0000000, 0xbe54650ced9eda1b],
    [0x3fe590c1c0000000, 0x3e693dd73be9b4c8],
    [0x3fe594d580000000, 0xbe50acbf03ae6fec],
    [0x3fe598e880000000, 0x3e6132b53edc6ff2],
    [0x3fe59cfb40000000, 0xbe6a05178208b948],
    [0x3fe5a10d40000000, 0xbe53b7ff59f6e0f0],
    [0x3fe5a51ec0000000, 0x3e2cfb5f3b8b9610],
    [0x3fe5a92fc0000000, 0x3e523f6b74fb6c86],
    [0x3fe5ad4040000000, 0x3e586b3e59f65354],
    [0x3fe5b15040000000, 0x3e56660a0534e354],
    [0x3fe5b55fc0000000, 0x3e48e5b4ac1a36e0],
    [0x3fe5b96ec0000000, 0xbe34ad795ac0cd10],
    [0x3fe5bd7d40000000, 0xbe5e31c719c80e48],
    [0x3fe5c18b40000000, 0xbe6f2ed03a571611],
    [0x3fe5c59880000000, 0x3e5a93bbbb5a05d4],
    [0x3fe5c9a580000000, 0xbe531b25b5ebc81c],
    [0x3fe5cdb1c0000000, 0x3e6c6c17648cf6e2],
    [0x3fe5d1bdc0000000, 0xbe14fec6b5ee4e20],
    [0x3fe5d5c900000000, 0x3e6f5764f01d569c],
    [0x3fe5d9d400000000, 0xbe3ba618f1dbdf60],
    [0x3fe5ddde40000000, 0x3e67149923773e8e],
    [0x3fe5e1e840000000, 0xbe60eb8b47e61fdb],
    [0x3fe5e5f180000000, 0x3e42aef89e4cc88c],
    [0x3fe5e9fa40000000, 0x3e67fb578517b07d],
    [0x3fe5ee02c0000000, 0xbe66dbe98a8f29f6],
    [0x3fe5f20a80000000, 0xbe4ee4ef2afe2dc4],
    [0x3fe5f611c0000000, 0x3e4610767d006e1b],
    [0x3fe5fa1880000000, 0x3e60fcd3327c90aa],
    [0x3fe5fe1ec0000000, 0x3e6ad189188d5d28],
    [0x3fe60224c0000000, 0xbe6cdd28d4378d73],
    [0x3fe6062a00000000, 0xbe65eeb7028e15bc],
    [0x3fe60a2ec0000000, 0xbe6042a224181d55],
    [0x3fe60e3300000000, 0xbe5770ee4e6b06ec],
    [0x3fe61236c0000000, 0xbe505f9e61c007f4],
    [0x3fe6163a00000000, 0xbe46213e5a2bc0bc],
    [0x3fe61a3cc0000000, 0xbe3d0d4ec035f3a8],
    [0x3fe61e3f00000000, 0xbe32dcdccb436e58],
    [0x3fe62240c0000000, 0xbe295ee9cf4ba620],
    [0x3fe6264200000000, 0xbe23078bd0f20710],
    [0x3fe62a42c0000000, 0xbe20b1be62caa3b0],
];

#[cfg(test)]
mod tests {
    use super::*;

    /// `java.util.Random` (48-bit LCG), enough for `nextDouble`.
    struct JavaRandom(u64);

    impl JavaRandom {
        fn new(seed: u64) -> Self {
            JavaRandom((seed ^ 0x5_DEEC_E66D) & ((1 << 48) - 1))
        }
        fn next(&mut self, bits: u32) -> u64 {
            self.0 = (self.0.wrapping_mul(0x5_DEEC_E66D).wrapping_add(0xB)) & ((1 << 48) - 1);
            self.0 >> (48 - bits)
        }
        fn next_double(&mut self) -> f64 {
            ((self.next(26) << 27) + self.next(27)) as f64 * (1.0 / (1u64 << 53) as f64)
        }
    }

    /// The hash the Java harness prints for `n` arguments drawn uniformly
    /// in `[lo, hi)` with `java.util.Random(seed)`: `h = h * 31 + bits`.
    fn hash(f: fn(f64) -> f64, seed: u64, n: usize, lo: f64, hi: f64) -> i64 {
        let mut r = JavaRandom::new(seed);
        let mut h = 0i64;
        for _ in 0..n {
            let x = lo + r.next_double() * (hi - lo);
            h = h.wrapping_mul(31).wrapping_add(f(x).to_bits() as i64);
        }
        h
    }

    /// One million arguments per case against the hashes the Java harness
    /// prints for `FastMath` in the jar (same draws, `java.util.Random(7)`).
    #[test]
    fn matches_commons_math_random() {
        type Case = (fn(f64) -> f64, f64, f64, i64);
        let cases: [Case; 9] = [
            (to_radians, -400.0, 400.0, -4_734_541_520_471_488_424),
            (sin, -4.0, 4.0, 7_169_289_807_337_132_420),
            (sin, -1e-4, 1e-4, 759_301_226_645_838_288),
            (cos, -4.0, 4.0, -8_555_055_292_584_637_077),
            (asin, -1.0, 1.0, 2_655_960_021_177_735_230),
            (asin, 0.0, 1e-4, -6_193_128_339_571_642_521),
            (|x| pow(x, 2.0), 0.0, 1e-3, -5_775_601_926_181_622_202),
            (|x| pow(x, 2.0), -2.0, 2.0, -4_792_390_756_575_133_861),
            (|x| pow(x, 2.5), 0.0, 100.0, -8_622_759_332_485_176_644),
        ];
        for (k, (f, lo, hi, want)) in cases.into_iter().enumerate() {
            assert_eq!(hash(f, 7, 1_000_000, lo, hi), want, "case {k}");
        }
    }

    /// Points of the Porto Alegre sample grid, where R5 snaps them and the
    /// distances it reports (R5's GeometryUtils.distance).
    #[test]
    fn haversine_of_recorded_snaps() {
        for (lat0, lon0, lat1, lon1, d) in [
            (
                -30.053_846_028_087_9,
                -51.158_245_946_603_301,
                -30.054_415_9,
                -51.158_365_3,
                64.363_765_011_787_265,
            ),
            (
                -30.100_576_010_026_7,
                -51.211_866_678_531_5,
                -30.101_055_6,
                -51.210_87,
                109.651_546_180_780_9,
            ),
            (
                -30.075_442_129_471_799,
                -51.165_206_385_026_799,
                -30.075_495_6,
                -51.165_189_9,
                6.150_187_418_561_043_5,
            ),
        ] {
            assert_eq!(
                haversine_m(lat0, lon0, lat1, lon1).to_bits(),
                f64::to_bits(d)
            );
        }
    }
}
