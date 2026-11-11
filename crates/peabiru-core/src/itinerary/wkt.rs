//! The WKT text r5r writes for an itinerary leg (`LineString.toString()`):
//! JTS 1.18.1's `WKTWriter`, whose ordinates go through `OrdinateFormat`,
//! a `java.text.DecimalFormat` with pattern `0`, at most 16 fraction
//! digits (the floating precision model's maximum significant digits) and
//! half-even rounding.
//!
//! `DecimalFormat` does not use `Double.toString`'s shortest digits (JDK 19
//! and later): in JDK 21, the version r5r runs on, `DigitList` still takes
//! the digits of the older `FloatingDecimal` conversion, which are not
//! always the shortest (`2e23` comes out as `1.9999999999999998E23`, hence
//! `199999999999999980000000`), and rounds them to 16 fraction digits with
//! the conversion's "rounded up" and "exact" flags. Both steps are
//! reproduced here as JDK 21 behaves; the arithmetic follows the published
//! descriptions of these steps, checked against the JVM on millions of
//! values.

use std::cmp::Ordering;

/// `LINESTRING (x y, x y, ...)`, or `LINESTRING EMPTY`.
pub(crate) fn line_wkt(line: &[[f64; 2]], out: &mut String) {
    if line.is_empty() {
        out.push_str("LINESTRING EMPTY");
        return;
    }
    out.push_str("LINESTRING (");
    for (i, p) in line.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        ordinate(p[0], out);
        out.push(' ');
        ordinate(p[1], out);
    }
    out.push(')');
}

/// The double a reader of the WKT gets back for `x` (sf in R, shapely in
/// Python): `x` itself unless JTS rounds it to 16 fraction digits.
pub(crate) fn ordinate_value(x: f64) -> f64 {
    let mut s = String::new();
    ordinate(x, &mut s);
    match s.as_str() {
        "NaN" => f64::NAN,
        "Inf" => f64::INFINITY,
        "-Inf" => f64::NEG_INFINITY,
        t => t.parse().expect("a decimal number"),
    }
}

/// `OrdinateFormat.format(x)` with 16 fraction digits.
pub(crate) fn ordinate(x: f64, out: &mut String) {
    const MAX_FRACTION: i32 = 16;
    if x.is_nan() {
        out.push_str("NaN");
        return;
    }
    if x.is_infinite() {
        out.push_str(if x > 0.0 { "Inf" } else { "-Inf" });
        return;
    }
    if x.is_sign_negative() {
        out.push('-');
    }
    let a = x.abs();
    // DigitList.set(isNegative, a, 16, fixedPoint = true)
    let (mut digits, mut at) = (Vec::new(), 0i32);
    if a != 0.0 {
        let fd = FloatingDecimal::of(a);
        digits = fd.digits[..fd.n].to_vec();
        at = fd.dec_exp;
        if -at > MAX_FRACTION {
            digits.clear();
        } else if -at == MAX_FRACTION {
            if should_round_up(&digits, 0, fd.rounded_up, fd.exact) {
                digits = vec![b'1'];
                at += 1;
            } else {
                digits.clear();
            }
        } else {
            while digits.len() > 1 && digits[digits.len() - 1] == b'0' {
                digits.pop();
            }
            let max = MAX_FRACTION + at;
            round(&mut digits, &mut at, max, fd.rounded_up, fd.exact);
        }
    }
    // DecimalFormat.subformat: integer digits (at least one), then the
    // fraction digits present, leading zeros included
    let mut k = 0usize;
    if at > 0 {
        for _ in 0..at {
            if k < digits.len() {
                out.push(digits[k] as char);
                k += 1;
            } else {
                out.push('0');
            }
        }
    } else {
        out.push('0');
    }
    if k < digits.len() {
        out.push('.');
        for i in 0..MAX_FRACTION {
            if k >= digits.len() {
                break;
            }
            if -1 - i > at - 1 {
                out.push('0');
            } else {
                out.push(digits[k] as char);
                k += 1;
            }
        }
    }
}

/// `DigitList.round(maximumDigits, ...)`.
fn round(digits: &mut Vec<u8>, at: &mut i32, max: i32, rounded_up: bool, exact: bool) {
    if max < 0 || max as usize >= digits.len() {
        return;
    }
    let mut m = max as usize;
    if should_round_up(digits, m, rounded_up, exact) {
        loop {
            if m == 0 {
                digits[0] = b'1';
                *at += 1;
                break;
            }
            m -= 1;
            digits[m] += 1;
            if digits[m] <= b'9' {
                break;
            }
        }
        m += 1;
    }
    digits.truncate(m);
    while digits.len() > 1 && digits[digits.len() - 1] == b'0' {
        digits.pop();
    }
}

/// `DigitList.shouldRoundUp` for `RoundingMode.HALF_EVEN`.
fn should_round_up(digits: &[u8], max: usize, rounded_up: bool, exact: bool) -> bool {
    if max >= digits.len() {
        return false;
    }
    match digits[max].cmp(&b'5') {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => {
            if max == digits.len() - 1 {
                if rounded_up {
                    false
                } else if !exact {
                    true
                } else {
                    max > 0 && (digits[max - 1] - b'0') % 2 != 0
                }
            } else {
                digits[max + 1..].iter().any(|&d| d != b'0')
            }
        }
    }
}

/// The digits of `FloatingDecimal.getBinaryToASCIIConverter(a)` for a
/// positive finite `a` (`BinaryToASCIIBuffer.dtoa`, compatible format):
/// `0.d1d2... × 10^dec_exp`.
struct FloatingDecimal {
    digits: [u8; 20],
    n: usize,
    dec_exp: i32,
    rounded_up: bool,
    exact: bool,
}

const EXP_SHIFT: i32 = 52;
const FRACT_HOB: u64 = 1 << 52;
const SIGNIF_MASK: u64 = FRACT_HOB - 1;
const N_5_BITS: [i32; 27] = [
    0, 3, 5, 7, 10, 12, 14, 17, 19, 21, 24, 26, 28, 31, 33, 35, 38, 40, 42, 45, 47, 49, 52, 54, 56,
    59, 61,
];
const INSIGNIFICANT_DIGITS: [i32; 64] = [
    0, 0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 5, 5, 5, 6, 6, 6, 6, 7, 7, 7, 8, 8, 8, 9, 9,
    9, 9, 10, 10, 10, 11, 11, 11, 12, 12, 12, 12, 13, 13, 13, 14, 14, 14, 15, 15, 15, 15, 16, 16,
    16, 17, 17, 17, 18, 18, 18, 19,
];
/// Entries of `FDBigInteger.LONG_5_POW` (5^0 to 5^26).
const LONG_5_POW_LEN: i32 = 27;

fn pow5_u64(p: i32) -> u64 {
    5u64.pow(p as u32)
}

impl FloatingDecimal {
    fn of(a: f64) -> FloatingDecimal {
        let bits = a.to_bits();
        let mut fract = bits & SIGNIF_MASK;
        let mut bin_exp = ((bits >> EXP_SHIFT) & 0x7ff) as i32;
        let n_significant;
        if bin_exp == 0 {
            let lz = fract.leading_zeros() as i32;
            let shift = lz - (63 - EXP_SHIFT);
            fract <<= shift;
            bin_exp = 1 - shift;
            n_significant = 64 - lz;
        } else {
            fract |= FRACT_HOB;
            n_significant = EXP_SHIFT + 1;
        }
        bin_exp -= 1023;
        let mut fd = FloatingDecimal {
            digits: [b'0'; 20],
            n: 0,
            dec_exp: 0,
            rounded_up: false,
            exact: false,
        };
        fd.dtoa(bin_exp, fract, n_significant);
        fd
    }

    fn develop_long_digits(&mut self, mut dec_exp: i32, mut v: u64, insignificant: i32) {
        if insignificant != 0 {
            let pow10 = pow5_u64(insignificant) << insignificant;
            let residue = v % pow10;
            v /= pow10;
            dec_exp += insignificant;
            if residue >= pow10 >> 1 {
                v += 1;
            }
        }
        // the digits from the last, trailing zeros skipped
        let mut rev = Vec::with_capacity(20);
        let mut c = v % 10;
        v /= 10;
        while c == 0 {
            dec_exp += 1;
            c = v % 10;
            v /= 10;
        }
        while v != 0 {
            rev.push(b'0' + c as u8);
            dec_exp += 1;
            c = v % 10;
            v /= 10;
        }
        rev.push(b'0' + c as u8);
        self.n = rev.len();
        for (i, d) in rev.iter().rev().enumerate() {
            self.digits[i] = *d;
        }
        self.dec_exp = dec_exp + 1;
    }

    fn dtoa(&mut self, bin_exp: i32, mut fract: u64, n_significant: i32) {
        let tail_zeros = fract.trailing_zeros() as i32;
        let n_fract_bits = EXP_SHIFT + 1 - tail_zeros;
        let n_tiny_bits = (n_fract_bits - bin_exp - 1).max(0);
        // the small-integer case (with 10^nTinyBits scaling fitting a long,
        // which always holds when nTinyBits is 0)
        if (-21..=62).contains(&bin_exp)
            && n_tiny_bits < LONG_5_POW_LEN
            && n_fract_bits + N_5_BITS[n_tiny_bits as usize] < 64
            && n_tiny_bits == 0
        {
            let insignificant = if bin_exp > n_significant {
                insignificant_for_pow2(bin_exp - n_significant - 1)
            } else {
                0
            };
            if bin_exp >= EXP_SHIFT {
                fract <<= bin_exp - EXP_SHIFT;
            } else {
                fract >>= EXP_SHIFT - bin_exp;
            }
            self.develop_long_digits(0, fract, insignificant);
            return;
        }
        let mut dec_exp = estimate_dec_exp(fract, bin_exp);
        let b5 = (-dec_exp).max(0);
        let mut b2 = b5 + n_tiny_bits + bin_exp;
        let s5 = dec_exp.max(0);
        let mut s2 = s5 + n_tiny_bits;
        let m5 = b5;
        let mut m2 = b2 - n_significant;
        fract >>= tail_zeros;
        b2 -= n_fract_bits - 1;
        let common2 = b2.min(s2);
        b2 -= common2;
        s2 -= common2;
        m2 -= common2;
        if n_fract_bits == 1 {
            m2 -= 1;
        }
        if m2 < 0 {
            b2 -= m2;
            s2 -= m2;
            m2 = 0;
        }
        let n5 = |p: i32| {
            if (p as usize) < N_5_BITS.len() {
                N_5_BITS[p as usize]
            } else {
                p * 3
            }
        };
        let b_bits = n_fract_bits + b2 + n5(b5);
        let ten_s_bits = s2 + 1 + n5(s5 + 1);
        let mut nd = 0usize;
        let (low, high, low_digit_difference): (bool, bool, i64);
        if b_bits < 64 && ten_s_bits < 64 {
            if b_bits < 32 && ten_s_bits < 32 {
                // int arithmetic, wrapping as Java's does
                let mut b = (fract as i32)
                    .wrapping_mul(pow5_u64(b5) as i32)
                    .wrapping_shl(b2 as u32);
                let s = (pow5_u64(s5) as i32).wrapping_shl(s2 as u32);
                let mut m = (pow5_u64(m5) as i32).wrapping_shl(m2 as u32);
                let tens = s.wrapping_mul(10);
                let mut q = b / s;
                b = 10i32.wrapping_mul(b % s);
                m = m.wrapping_mul(10);
                let mut lo = b < m;
                let mut hi = b.wrapping_add(m) > tens;
                if q == 0 && !hi {
                    dec_exp -= 1;
                } else {
                    self.digits[nd] = b'0' + q as u8;
                    nd += 1;
                }
                if !(-3..8).contains(&dec_exp) {
                    lo = false;
                    hi = false;
                }
                while !lo && !hi {
                    q = b / s;
                    b = 10i32.wrapping_mul(b % s);
                    m = m.wrapping_mul(10);
                    if m > 0 {
                        lo = b < m;
                        hi = b.wrapping_add(m) > tens;
                    } else {
                        lo = true;
                        hi = true;
                    }
                    self.digits[nd] = b'0' + q as u8;
                    nd += 1;
                }
                low_digit_difference = i64::from(b.wrapping_shl(1).wrapping_sub(tens));
                self.exact = b == 0;
                low = lo;
                high = hi;
            } else {
                // long arithmetic
                let mut b = (fract as i64)
                    .wrapping_mul(pow5_u64(b5) as i64)
                    .wrapping_shl(b2 as u32);
                let s = (pow5_u64(s5) as i64).wrapping_shl(s2 as u32);
                let mut m = (pow5_u64(m5) as i64).wrapping_shl(m2 as u32);
                let tens = s.wrapping_mul(10);
                let mut q = b / s;
                b = 10i64.wrapping_mul(b % s);
                m = m.wrapping_mul(10);
                let mut lo = b < m;
                let mut hi = b.wrapping_add(m) > tens;
                if q == 0 && !hi {
                    dec_exp -= 1;
                } else {
                    self.digits[nd] = b'0' + q as u8;
                    nd += 1;
                }
                if !(-3..8).contains(&dec_exp) {
                    lo = false;
                    hi = false;
                }
                while !lo && !hi {
                    q = b / s;
                    b = 10i64.wrapping_mul(b % s);
                    m = m.wrapping_mul(10);
                    if m > 0 {
                        lo = b < m;
                        hi = b.wrapping_add(m) > tens;
                    } else {
                        lo = true;
                        hi = true;
                    }
                    self.digits[nd] = b'0' + q as u8;
                    nd += 1;
                }
                low_digit_difference = b.wrapping_shl(1).wrapping_sub(tens);
                self.exact = b == 0;
                low = lo;
                high = hi;
            }
        } else {
            // exact big-integer arithmetic (common factors of 2 do not
            // change the comparisons, so FDBigInteger's normalisation
            // shift is left out)
            let s = Big::pow52(s5, s2);
            let mut b = Big::from_u64(fract).mul_pow52(b5, b2);
            let mut m = Big::pow52(m5 + 1, m2 + 1);
            let ten_s = Big::pow52(s5 + 1, s2 + 1);
            let mut q = b.quo_rem_iteration(&s);
            let mut lo = b.cmp(&m) == Ordering::Less;
            let mut hi = b.add(&m).cmp(&ten_s) != Ordering::Less;
            if q == 0 && !hi {
                dec_exp -= 1;
            } else {
                self.digits[nd] = b'0' + q;
                nd += 1;
            }
            if !(-3..8).contains(&dec_exp) {
                lo = false;
                hi = false;
            }
            while !lo && !hi {
                q = b.quo_rem_iteration(&s);
                m = m.mul_small(10);
                lo = b.cmp(&m) == Ordering::Less;
                hi = b.add(&m).cmp(&ten_s) != Ordering::Less;
                self.digits[nd] = b'0' + q;
                nd += 1;
            }
            low_digit_difference = if hi && lo {
                match b.mul_small(2).cmp(&ten_s) {
                    Ordering::Less => -1,
                    Ordering::Equal => 0,
                    Ordering::Greater => 1,
                }
            } else {
                0
            };
            self.exact = b.is_zero();
            low = lo;
            high = hi;
        }
        self.dec_exp = dec_exp + 1;
        self.n = nd;
        if high {
            if low {
                if low_digit_difference == 0 {
                    if (self.digits[nd - 1] - b'0') & 1 != 0 {
                        self.roundup();
                    }
                } else if low_digit_difference > 0 {
                    self.roundup();
                }
            } else {
                self.roundup();
            }
        }
    }

    fn roundup(&mut self) {
        let mut i = self.n - 1;
        let mut q = self.digits[i];
        if q == b'9' {
            while q == b'9' && i > 0 {
                self.digits[i] = b'0';
                i -= 1;
                q = self.digits[i];
            }
            if q == b'9' {
                // carry out: a leading 1, one more in the exponent (the
                // flag is left as it was, as in the JDK)
                self.dec_exp += 1;
                self.digits[0] = b'1';
                return;
            }
        }
        self.digits[i] = q + 1;
        self.rounded_up = true;
    }
}

fn insignificant_for_pow2(p2: i32) -> i32 {
    if p2 > 1 && (p2 as usize) < INSIGNIFICANT_DIGITS.len() {
        INSIGNIFICANT_DIGITS[p2 as usize]
    } else {
        0
    }
}

/// `estimateDecExp`: floor(log10(d)), estimated (never too low). The
/// constants are the JDK's literals (its log10(2) is not `LOG10_2`).
#[allow(clippy::approx_constant)]
fn estimate_dec_exp(fract: u64, bin_exp: i32) -> i32 {
    let d2 = f64::from_bits((1023u64 << EXP_SHIFT) | (fract & SIGNIF_MASK));
    let d = (d2 - 1.5) * 0.289529654 + 0.176091259 + f64::from(bin_exp) * 0.301029995663981;
    let bits = d.to_bits();
    let exponent = ((bits >> EXP_SHIFT) & 0x7ff) as i32 - 1023;
    let negative = bits >> 63 != 0;
    if (0..52).contains(&exponent) {
        let mask = SIGNIF_MASK >> exponent;
        let r = (((bits & SIGNIF_MASK) | FRACT_HOB) >> (EXP_SHIFT - exponent)) as i32;
        if negative {
            if mask & bits == 0 {
                -r
            } else {
                -r - 1
            }
        } else {
            r
        }
    } else if exponent < 0 {
        if bits & !(1u64 << 63) == 0 {
            0
        } else if negative {
            -1
        } else {
            0
        }
    } else {
        d as i32
    }
}

/// A non-negative big integer, 32-bit limbs, least significant first.
#[derive(Clone)]
struct Big(Vec<u32>);

impl Big {
    fn from_u64(v: u64) -> Big {
        let mut b = Big(vec![v as u32, (v >> 32) as u32]);
        b.trim();
        b
    }

    fn pow52(p5: i32, p2: i32) -> Big {
        Big::from_u64(1).mul_pow52(p5, p2)
    }

    fn trim(&mut self) {
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    fn is_zero(&self) -> bool {
        self.0.is_empty()
    }

    fn mul_small(&self, k: u32) -> Big {
        let mut out = Vec::with_capacity(self.0.len() + 1);
        let mut carry = 0u64;
        for &d in &self.0 {
            let t = u64::from(d) * u64::from(k) + carry;
            out.push(t as u32);
            carry = t >> 32;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        Big(out)
    }

    fn mul_pow52(self, p5: i32, p2: i32) -> Big {
        let mut b = self;
        let mut p = p5;
        while p >= 13 {
            b = b.mul_small(1_220_703_125); // 5^13
            p -= 13;
        }
        if p > 0 {
            b = b.mul_small(5u32.pow(p as u32));
        }
        b.shl(p2.max(0) as u32)
    }

    fn shl(self, n: u32) -> Big {
        if self.is_zero() {
            return self;
        }
        let (words, bits) = ((n / 32) as usize, n % 32);
        let mut out = vec![0u32; words];
        if bits == 0 {
            out.extend_from_slice(&self.0);
        } else {
            let mut carry = 0u32;
            for &d in &self.0 {
                out.push((d << bits) | carry);
                carry = d >> (32 - bits);
            }
            if carry != 0 {
                out.push(carry);
            }
        }
        Big(out)
    }

    fn add(&self, o: &Big) -> Big {
        let n = self.0.len().max(o.0.len());
        let mut out = Vec::with_capacity(n + 1);
        let mut carry = 0u64;
        for i in 0..n {
            let t = u64::from(*self.0.get(i).unwrap_or(&0))
                + u64::from(*o.0.get(i).unwrap_or(&0))
                + carry;
            out.push(t as u32);
            carry = t >> 32;
        }
        if carry != 0 {
            out.push(carry as u32);
        }
        Big(out)
    }

    /// `self -= o`, for `self >= o`.
    fn sub_assign(&mut self, o: &Big) {
        let mut borrow = 0i64;
        for i in 0..self.0.len() {
            let t = i64::from(self.0[i]) - i64::from(*o.0.get(i).unwrap_or(&0)) - borrow;
            if t < 0 {
                self.0[i] = (t + (1i64 << 32)) as u32;
                borrow = 1;
            } else {
                self.0[i] = t as u32;
                borrow = 0;
            }
        }
        self.trim();
    }

    fn cmp(&self, o: &Big) -> Ordering {
        self.0
            .len()
            .cmp(&o.0.len())
            .then_with(|| self.0.iter().rev().cmp(o.0.iter().rev()))
    }

    /// `FDBigInteger.quoRemIteration`: `q = self / s` (a digit), then
    /// `self = 10 * (self % s)`.
    fn quo_rem_iteration(&mut self, s: &Big) -> u8 {
        let mut q = 0u8;
        while self.cmp(s) != Ordering::Less {
            self.sub_assign(s);
            q += 1;
        }
        *self = self.mul_small(10);
        self.trim();
        q
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(x: f64) -> String {
        let mut s = String::new();
        ordinate(x, &mut s);
        s
    }

    #[test]
    fn ordinates_as_jts_writes_them_on_jdk21() {
        // printed by JTS 1.18.1 (R5 7.6) on JDK 21
        assert_eq!(fmt(-51.2345678), "-51.2345678");
        assert_eq!(fmt(-30.0), "-30");
        assert_eq!(fmt(0.1 + 0.2), "0.3");
        assert_eq!(fmt(1e-20), "0");
        assert_eq!(fmt(2e23), "199999999999999980000000");
        assert_eq!(fmt(-0.0), "-0");
        assert_eq!(fmt(1e7), "10000000");
        assert_eq!(fmt(123456789.0), "123456789");
        assert_eq!(fmt(0.001), "0.001");
        assert_eq!(fmt(5e-324), "0");
        // and read back by sf
        assert_eq!(ordinate_value(-51.123456789012345), -51.123456789012345);
        assert_eq!(ordinate_value(0.1 + 0.2), 0.3);
        assert_eq!(ordinate_value(1.0 / 3.0), 1.0 / 3.0);
        assert_eq!(
            ordinate_value(-0.000012345678901234567),
            -0.0000123456789012
        );
        assert_eq!(ordinate_value(1e-20), 0.0);
        assert_eq!(ordinate_value(5e-8), 5e-8);
        assert_eq!(ordinate_value(3e-7), 3e-7);
        let mut s = String::new();
        line_wkt(&[[-51.5, -30.25], [1.0, 2.0]], &mut s);
        assert_eq!(s, "LINESTRING (-51.5 -30.25, 1 2)");
        s.clear();
        line_wkt(&[], &mut s);
        assert_eq!(s, "LINESTRING EMPTY");
    }

    /// Checks a file of `<hex bits>\t<JTS text>` lines written by the JVM
    /// (path in `PEABIRU_JTS_ORDINATES`).
    #[test]
    #[ignore]
    fn ordinates_from_file() {
        let path = std::env::var("PEABIRU_JTS_ORDINATES").expect("PEABIRU_JTS_ORDINATES");
        let text = std::fs::read_to_string(path).expect("readable");
        let (mut n, mut bad) = (0, 0);
        for line in text.lines() {
            let (h, want) = line.split_once('\t').expect("two fields");
            let x = f64::from_bits(u64::from_str_radix(h, 16).expect("hex"));
            let got = fmt(x);
            n += 1;
            if got != want {
                bad += 1;
                if bad <= 20 {
                    eprintln!("{h} {x:e}: got {got}, JTS {want}");
                }
            }
        }
        eprintln!("{n} values, {bad} differ");
        assert_eq!(bad, 0);
    }
}
