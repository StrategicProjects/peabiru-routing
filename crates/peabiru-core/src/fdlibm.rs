//! The functions of `java.lang.StrictMath` that the coordinate transforms
//! of GeoTools call through `java.lang.Math` (sine, cosine, tangent,
//! arc tangent, natural logarithm), ported from fdlibm 5.3, which
//! `StrictMath` specifies. They are pure IEEE double arithmetic, so they
//! give the same bits on every platform (the system's `sin` and `cos`
//! differ between platforms in the last bit).
//!
//! fdlibm is Copyright (C) 1993 by Sun Microsystems, Inc.: "Developed at
//! SunSoft, a Sun Microsystems, Inc. business. Permission to use, copy,
//! modify, and distribute this software is freely granted, provided that
//! this notice is preserved."
//!
//! Only arguments in the range the transforms use are reduced the fdlibm
//! way (|x| up to 2^19 pi/2, far beyond the radians of any longitude); a
//! larger argument gives NaN.

#![allow(
    clippy::excessive_precision,
    clippy::many_single_char_names,
    clippy::approx_constant,
    clippy::needless_late_init
)] // fdlibm's constants and layout, as written

#[inline]
fn hi(x: f64) -> i32 {
    (x.to_bits() >> 32) as i32
}

#[inline]
fn lo(x: f64) -> u32 {
    x.to_bits() as u32
}

#[inline]
fn with_hi_lo(h: i32, l: u32) -> f64 {
    f64::from_bits((u64::from(h as u32) << 32) | u64::from(l))
}

/// `__kernel_sin(x, y, iy)` on `[-pi/4, pi/4]`.
fn k_sin(x: f64, y: f64, iy: bool) -> f64 {
    const S1: f64 = -1.66666666666666324348e-01;
    const S2: f64 = 8.33333333332248946124e-03;
    const S3: f64 = -1.98412698298579493134e-04;
    const S4: f64 = 2.75573137070700676789e-06;
    const S5: f64 = -2.50507602534068634195e-08;
    const S6: f64 = 1.58969099521155010221e-10;
    let ix = hi(x) & 0x7fff_ffff;
    if ix < 0x3e40_0000 && (x as i32) == 0 {
        return x;
    }
    let z = x * x;
    let v = z * x;
    let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
    if !iy {
        x + v * (S1 + z * r)
    } else {
        x - ((z * (0.5 * y - v * r) - y) - v * S1)
    }
}

/// `__kernel_cos(x, y)` on `[-pi/4, pi/4]`.
fn k_cos(x: f64, y: f64) -> f64 {
    const C1: f64 = 4.16666666666666019037e-02;
    const C2: f64 = -1.38888888888741095749e-03;
    const C3: f64 = 2.48015872894767294178e-05;
    const C4: f64 = -2.75573143513906633035e-07;
    const C5: f64 = 2.08757232129817482790e-09;
    const C6: f64 = -1.13596475577881948265e-11;
    let ix = hi(x) & 0x7fff_ffff;
    if ix < 0x3e40_0000 && (x as i32) == 0 {
        return 1.0;
    }
    let z = x * x;
    let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
    if ix < 0x3fd3_3333 {
        1.0 - (0.5 * z - (z * r - x * y))
    } else {
        let qx = if ix > 0x3fe9_0000 {
            0.28125
        } else {
            with_hi_lo(ix - 0x0020_0000, 0)
        };
        let hz = 0.5 * z - qx;
        let a = 1.0 - qx;
        a - (hz - (z * r - x * y))
    }
}

/// `__ieee754_rem_pio2` for |x| up to about 2^19 pi/2: `x - n pi/2` as
/// `(y0, y1)` and `n`; `None` beyond.
fn rem_pio2(x: f64) -> Option<(i32, f64, f64)> {
    const NPIO2_HW: [i32; 32] = [
        0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB,
        0x402921FB, 0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB,
        0x40378FDB, 0x403921FB, 0x403AB41B, 0x403C463A, 0x403DD85A, 0x403F6A7A, 0x40407E4C,
        0x4041475C, 0x4042106C, 0x4042D97C, 0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB,
        0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB,
    ];
    const INVPIO2: f64 = 6.36619772367581382433e-01;
    const PIO2_1: f64 = 1.57079632673412561417e+00;
    const PIO2_1T: f64 = 6.07710050650619224932e-11;
    const PIO2_2: f64 = 6.07710050630396597660e-11;
    const PIO2_2T: f64 = 2.02226624879595063154e-21;
    const PIO2_3: f64 = 2.02226624871116645580e-21;
    const PIO2_3T: f64 = 8.47842766036889956997e-32;
    let hx = hi(x);
    let ix = hx & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        return Some((0, x, 0.0));
    }
    if ix < 0x4002_d97c {
        // |x| < 3pi/4: one subtraction of pi/2
        return Some(if hx > 0 {
            let mut z = x - PIO2_1;
            if ix != 0x3ff9_21fb {
                let y0 = z - PIO2_1T;
                (1, y0, (z - y0) - PIO2_1T)
            } else {
                z -= PIO2_2;
                let y0 = z - PIO2_2T;
                (1, y0, (z - y0) - PIO2_2T)
            }
        } else {
            let mut z = x + PIO2_1;
            if ix != 0x3ff9_21fb {
                let y0 = z + PIO2_1T;
                (-1, y0, (z - y0) + PIO2_1T)
            } else {
                z += PIO2_2;
                let y0 = z + PIO2_2T;
                (-1, y0, (z - y0) + PIO2_2T)
            }
        });
    }
    if ix > 0x4139_21fb {
        return None;
    }
    let t = x.abs();
    let n = (t * INVPIO2 + 0.5) as i32;
    let fnum = f64::from(n);
    let mut r = t - fnum * PIO2_1;
    let mut w = fnum * PIO2_1T;
    let y0;
    if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
        y0 = r - w;
    } else {
        let j = ix >> 20;
        let mut y = r - w;
        let mut i = j - ((hi(y) >> 20) & 0x7ff);
        if i > 16 {
            let t = r;
            w = fnum * PIO2_2;
            r = t - w;
            w = fnum * PIO2_2T - ((t - r) - w);
            y = r - w;
            i = j - ((hi(y) >> 20) & 0x7ff);
            if i > 49 {
                let t = r;
                w = fnum * PIO2_3;
                r = t - w;
                w = fnum * PIO2_3T - ((t - r) - w);
                y = r - w;
            }
        }
        y0 = y;
    }
    let y1 = (r - y0) - w;
    Some(if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) })
}

/// `StrictMath.sin`.
pub(crate) fn sin(x: f64) -> f64 {
    let ix = hi(x) & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        return k_sin(x, 0.0, false);
    }
    if ix >= 0x7ff0_0000 {
        return f64::NAN;
    }
    match rem_pio2(x) {
        Some((n, y0, y1)) => match n & 3 {
            0 => k_sin(y0, y1, true),
            1 => k_cos(y0, y1),
            2 => -k_sin(y0, y1, true),
            _ => -k_cos(y0, y1),
        },
        None => f64::NAN,
    }
}

/// `StrictMath.cos`.
pub(crate) fn cos(x: f64) -> f64 {
    let ix = hi(x) & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        return k_cos(x, 0.0);
    }
    if ix >= 0x7ff0_0000 {
        return f64::NAN;
    }
    match rem_pio2(x) {
        Some((n, y0, y1)) => match n & 3 {
            0 => k_cos(y0, y1),
            1 => -k_sin(y0, y1, true),
            2 => -k_cos(y0, y1),
            _ => k_sin(y0, y1, true),
        },
        None => f64::NAN,
    }
}

/// `__kernel_tan(x, y, k)` on `[-pi/4, pi/4]`: tan if `k == 1`, -1/tan
/// if `k == -1` (fdlibm 5.3, with the fix for arguments near pi/4).
fn k_tan(mut x: f64, mut y: f64, iy: i32) -> f64 {
    const T: [f64; 13] = [
        3.33333333333334091986e-01,
        1.33333333333201242699e-01,
        5.39682539762260521377e-02,
        2.18694882948595424599e-02,
        8.86323982359930005737e-03,
        3.59207910759131235356e-03,
        1.45620945432529025516e-03,
        5.88041240820264096874e-04,
        2.46463134818469906812e-04,
        7.81794442939557092300e-05,
        7.14072491382608190305e-05,
        -1.85586374855275456654e-05,
        2.59073051863633712884e-05,
    ];
    const PIO4: f64 = 7.85398163397448278999e-01;
    const PIO4LO: f64 = 3.06161699786838301793e-17;
    let hx = hi(x);
    let ix = hx & 0x7fff_ffff;
    if ix < 0x3e30_0000 && (x as i32) == 0 {
        if ((ix as u32) | lo(x)) | ((iy + 1) as u32) == 0 {
            return 1.0 / x.abs();
        } else if iy == 1 {
            return x;
        } else {
            // compute -1/(x+y) carefully
            let w = x + y;
            let z = with_hi_lo(hi(w), 0);
            let v = y - (z - x);
            let a = -1.0 / w;
            let t = with_hi_lo(hi(a), 0);
            let s = 1.0 + t * z;
            return t + a * (s + t * v);
        }
    }
    let big = ix >= 0x3fe5_9428;
    if big {
        if hx < 0 {
            x = -x;
            y = -y;
        }
        let z = PIO4 - x;
        let w = PIO4LO - y;
        x = z + w;
        y = 0.0;
    }
    let z = x * x;
    let w = z * z;
    let r = T[1] + w * (T[3] + w * (T[5] + w * (T[7] + w * (T[9] + w * T[11]))));
    let v = z * (T[2] + w * (T[4] + w * (T[6] + w * (T[8] + w * (T[10] + w * T[12])))));
    let s = z * x;
    let r = y + z * (s * (r + v) + y);
    let r = r + T[0] * s;
    let w = x + r;
    if big {
        let v = f64::from(iy);
        let sign = f64::from(1 - ((hx >> 30) & 2));
        return sign * (v - 2.0 * (x - (w * w / (w + v) - r)));
    }
    if iy == 1 {
        w
    } else {
        // -1/(x+r) with care
        let z = with_hi_lo(hi(w), 0);
        let v = r - (z - x);
        let a = -1.0 / w;
        let t = with_hi_lo(hi(a), 0);
        let s = 1.0 + t * z;
        t + a * (s + t * v)
    }
}

/// `StrictMath.tan`.
pub(crate) fn tan(x: f64) -> f64 {
    let ix = hi(x) & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        return k_tan(x, 0.0, 1);
    }
    if ix >= 0x7ff0_0000 {
        return f64::NAN;
    }
    match rem_pio2(x) {
        Some((n, y0, y1)) => k_tan(y0, y1, 1 - ((n & 1) << 1)),
        None => f64::NAN,
    }
}

/// `StrictMath.atan`.
pub(crate) fn atan(x: f64) -> f64 {
    const ATANHI: [f64; 4] = [
        4.63647609000806093515e-01,
        7.85398163397448278999e-01,
        9.82793723247329054082e-01,
        1.57079632679489655800e+00,
    ];
    const ATANLO: [f64; 4] = [
        2.26987774529616870924e-17,
        3.06161699786838301793e-17,
        1.39033110312309984516e-17,
        6.12323399573676603587e-17,
    ];
    const AT: [f64; 11] = [
        3.33333333333329318027e-01,
        -1.99999999998764832476e-01,
        1.42857142725034663711e-01,
        -1.11111104054623557880e-01,
        9.09088713343650656196e-02,
        -7.69187620504482999495e-02,
        6.66107313738753120669e-02,
        -5.83357013379057348645e-02,
        4.97687799461593236017e-02,
        -3.65315727442169155270e-02,
        1.62858201153657823623e-02,
    ];
    let hx = hi(x);
    let ix = hx & 0x7fff_ffff;
    if ix >= 0x4410_0000 {
        // |x| >= 2^66
        if ix > 0x7ff0_0000 || (ix == 0x7ff0_0000 && lo(x) != 0) {
            return x + x; // NaN
        }
        return if hx > 0 {
            ATANHI[3] + ATANLO[3]
        } else {
            -ATANHI[3] - ATANLO[3]
        };
    }
    let id: i32;
    let mut x = x;
    if ix < 0x3fdc_0000 {
        // |x| < 0.4375
        if ix < 0x3e20_0000 {
            // |x| < 2^-29 (the "raise inexact" test always passes)
            return x;
        }
        id = -1;
    } else {
        x = x.abs();
        if ix < 0x3ff3_0000 {
            if ix < 0x3fe6_0000 {
                id = 0;
                x = (2.0 * x - 1.0) / (2.0 + x);
            } else {
                id = 1;
                x = (x - 1.0) / (x + 1.0);
            }
        } else if ix < 0x4003_8000 {
            id = 2;
            x = (x - 1.5) / (1.0 + 1.5 * x);
        } else {
            id = 3;
            x = -1.0 / x;
        }
    }
    let z = x * x;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    if id < 0 {
        return x - x * (s1 + s2);
    }
    let i = id as usize;
    let z = ATANHI[i] - ((x * (s1 + s2) - ATANLO[i]) - x);
    if hx < 0 {
        -z
    } else {
        z
    }
}

/// `StrictMath.atan2(y, x)`.
pub(crate) fn atan2(y: f64, x: f64) -> f64 {
    const PI_O_4: f64 = 7.8539816339744827900e-01;
    const PI_O_2: f64 = 1.5707963267948965580e+00;
    const PI: f64 = 3.1415926535897931160e+00;
    const PI_LO: f64 = 1.2246467991473531772e-16;
    let hx = hi(x);
    let ix = hx & 0x7fff_ffff;
    let lx = lo(x);
    let hy = hi(y);
    let iy = hy & 0x7fff_ffff;
    let ly = lo(y);
    if ((ix as u32) | ((lx | lx.wrapping_neg()) >> 31)) > 0x7ff0_0000
        || ((iy as u32) | ((ly | ly.wrapping_neg()) >> 31)) > 0x7ff0_0000
    {
        return x + y; // NaN
    }
    if ((hx.wrapping_sub(0x3ff0_0000)) as u32 | lx) == 0 {
        return atan(y); // x = 1
    }
    let m = ((hy >> 31) & 1) | ((hx >> 30) & 2);
    if ((iy as u32) | ly) == 0 {
        return match m {
            0 | 1 => y,
            2 => PI,
            _ => -PI,
        };
    }
    if ((ix as u32) | lx) == 0 {
        return if hy < 0 { -PI_O_2 } else { PI_O_2 };
    }
    if ix == 0x7ff0_0000 {
        return if iy == 0x7ff0_0000 {
            match m {
                0 => PI_O_4,
                1 => -PI_O_4,
                2 => 3.0 * PI_O_4,
                _ => -3.0 * PI_O_4,
            }
        } else {
            match m {
                0 => 0.0,
                1 => -0.0,
                2 => PI,
                _ => -PI,
            }
        };
    }
    if iy == 0x7ff0_0000 {
        return if hy < 0 { -PI_O_2 } else { PI_O_2 };
    }
    let k = (iy - ix) >> 20;
    let z = if k > 60 {
        PI_O_2 + 0.5 * PI_LO
    } else if hx < 0 && k < -60 {
        0.0
    } else {
        atan((y / x).abs())
    };
    match m {
        0 => z,
        1 => -z,
        2 => PI - (z - PI_LO),
        _ => (z - PI_LO) - PI,
    }
}

/// `StrictMath.log` (`__ieee754_log`).
pub(crate) fn log(x: f64) -> f64 {
    const LN2_HI: f64 = 6.93147180369123816490e-01;
    const LN2_LO: f64 = 1.90821492927058770002e-10;
    const TWO54: f64 = 1.80143985094819840000e+16;
    const LG1: f64 = 6.666666666666735130e-01;
    const LG2: f64 = 3.999999999940941908e-01;
    const LG3: f64 = 2.857142874366239149e-01;
    const LG4: f64 = 2.222219843214978396e-01;
    const LG5: f64 = 1.818357216161805012e-01;
    const LG6: f64 = 1.531383769920937332e-01;
    const LG7: f64 = 1.479819860511658591e-01;
    let mut hx = hi(x);
    let lx = lo(x);
    let mut k = 0i32;
    let mut x = x;
    if hx < 0x0010_0000 {
        if ((hx & 0x7fff_ffff) as u32 | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = hi(x);
    }
    if hx >= 0x7ff0_0000 {
        return x + x;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000f_ffff;
    let i = (hx + 0x95f64) & 0x0010_0000;
    x = with_hi_lo(hx | (i ^ 0x3ff0_0000), lo(x));
    k += i >> 20;
    let f = x - 1.0;
    if (0x000f_ffff & (2 + hx)) < 3 {
        // |f| < 2^-20
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            let dk = f64::from(k);
            return dk * LN2_HI + dk * LN2_LO;
        }
        let r = f * f * (0.5 - 0.33333333333333333 * f);
        if k == 0 {
            return f - r;
        }
        let dk = f64::from(k);
        return dk * LN2_HI - ((r - dk * LN2_LO) - f);
    }
    let s = f / (2.0 + f);
    let dk = f64::from(k);
    let z = s * s;
    let mut i = hx - 0x6147a;
    let w = z * z;
    let j = 0x6b851 - hx;
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            f - (hfsq - s * (hfsq + r))
        } else {
            dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)
        }
    } else if k == 0 {
        f - s * (f - r)
    } else {
        dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `StrictMath` (OpenJDK 21) at a few arguments covering the reduction
    /// paths (none, one subtraction of pi/2, the medium range); the port
    /// was compared on two million arguments with the JVM.
    #[test]
    fn values_of_strictmath() {
        for (x, s, c) in [
            (0.5f64, 0.479425538604203f64, 0.8775825618903728f64),
            (2.1686667900494188, 0.8265361694784393, -0.5628836118985067),
            (-3.593994800278316, 0.4371272831140328, -0.899399654412511),
            (6.468111018232092, 0.18387351172100821, 0.9829499131122522),
        ] {
            assert_eq!(sin(x), s, "sin {x}");
            assert_eq!(cos(x), c, "cos {x}");
        }
        assert_eq!(atan2(1.0, -1.0), 2.356194490192345);
        assert_eq!(atan(0.5), 0.4636476090008061);
        assert_eq!(tan(1.2), 2.5721516221263188);
        assert_eq!(log(10.0), 2.302585092994046);
        assert!(sin(f64::NAN).is_nan() && cos(f64::INFINITY).is_nan());
        assert!(sin(1e300).is_nan()); // beyond the reduction kept here
        assert_eq!(sin(-0.0).to_bits(), (-0.0f64).to_bits());
        assert_eq!(log(0.0), f64::NEG_INFINITY);
        assert!(log(-1.0).is_nan());
    }
}
