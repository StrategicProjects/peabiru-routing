//! Java's number parsing as gtfs-lib uses it (`Integer.parseInt`,
//! `Double.parseDouble`), for reading values exactly as R5 reads them.

/// gtfs-lib's `Entity.INT_MISSING`: the value of an int field that could
/// not be read.
pub const INT_MISSING: i32 = i32::MIN;

/// Java `Integer.parseInt(s)`: an optional sign and ASCII digits, within
/// the `int` range. (Java also takes digits of other scripts, such as
/// Arabic-Indic digits; they are refused here.)
pub fn parse_int(s: &str) -> Option<i32> {
    let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Java `Double.parseDouble(s)`: leading and trailing characters up to
/// U+0020 are ignored; then an optional sign and `NaN`, `Infinity`, a
/// decimal number (`1`, `1.`, `.5`, `1e-3`) with an optional `f`, `F`,
/// `d` or `D` suffix, or a hexadecimal one (`0x1.8p3`).
pub fn parse_double(s: &str) -> Option<f64> {
    let s = s.trim_matches(|c: char| c <= ' ');
    let (neg, body) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let v = match body {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        _ => {
            let body = body.strip_suffix(['f', 'F', 'd', 'D']).unwrap_or(body);
            if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
                parse_hex(hex)?
            } else {
                parse_decimal(body)?
            }
        }
    };
    Some(if neg { -v } else { v })
}

/// `digits [. digits] [e [sign] digits]`, with digits on at least one side
/// of the point.
fn parse_decimal(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    let mut i = 0;
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut n_digits = i - int_start;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let frac_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        n_digits += i - frac_start;
    }
    if n_digits == 0 {
        return None;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let exp_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return None;
        }
    }
    if i != b.len() {
        return None;
    }
    s.parse().ok()
}

/// `hexdigits [. hexdigits] p [sign] digits`, the binary exponent being
/// required. Rounded through `f64` arithmetic, which is exact for up to 13
/// hexadecimal digits (Java rounds longer mantissas correctly; this may
/// not, in the last bit).
fn parse_hex(s: &str) -> Option<f64> {
    let (mant, exp) = s.split_once(['p', 'P'])?;
    let exp: i32 = {
        let d = exp.strip_prefix(['+', '-']).unwrap_or(exp);
        if d.is_empty() || !d.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        exp.parse().unwrap_or(if exp.starts_with('-') {
            i32::MIN
        } else {
            i32::MAX
        })
    };
    let (int, frac) = mant.split_once('.').unwrap_or((mant, ""));
    if int.is_empty() && frac.is_empty() {
        return None;
    }
    let mut v = 0.0f64;
    for c in int.chars() {
        v = v * 16.0 + f64::from(c.to_digit(16)?);
    }
    let mut scale = 1.0f64;
    for c in frac.chars() {
        scale /= 16.0;
        v += f64::from(c.to_digit(16)?) * scale;
    }
    Some(v * 2f64.powi(exp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ints_as_java() {
        assert_eq!(parse_int("42"), Some(42));
        assert_eq!(parse_int("+7"), Some(7));
        assert_eq!(parse_int("-0"), Some(0));
        assert_eq!(parse_int("2147483648"), None);
        assert_eq!(parse_int(" 1"), None);
        assert_eq!(parse_int("1.0"), None);
        assert_eq!(parse_int("+"), None);
        assert_eq!(parse_int(""), None);
    }

    #[test]
    fn doubles_as_java() {
        assert_eq!(parse_double("-30.05"), Some(-30.05));
        assert_eq!(parse_double(" 1.5 "), Some(1.5));
        assert_eq!(parse_double("1."), Some(1.0));
        assert_eq!(parse_double(".5"), Some(0.5));
        assert_eq!(parse_double("2e3"), Some(2000.0));
        assert_eq!(parse_double("1.5f"), Some(1.5));
        assert_eq!(parse_double("1.5D"), Some(1.5));
        assert_eq!(parse_double("-Infinity"), Some(f64::NEG_INFINITY));
        assert!(parse_double("NaN").unwrap().is_nan());
        assert_eq!(parse_double("0x1.8p1"), Some(3.0));
        for bad in [
            "", ".", "e5", "1e", "inf", "nan", "infinity", "1,5", "1.5ff", "0x1.8", "--1",
        ] {
            assert_eq!(parse_double(bad), None, "{bad}");
        }
    }
}
