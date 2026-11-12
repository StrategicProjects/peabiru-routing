//! Argument and input handling shared by the subcommands: every bad input
//! ends the process with a one-line message on stderr that names the
//! argument, or the file and line, at fault. Exit status 2 is a usage error
//! (a bad argument, or parameters the engine rejects), 1 a runtime failure
//! (a file that cannot be read, parsed or written, a network that cannot
//! be built).

use std::fmt::Display;
use std::str::FromStr;

/// Exit with status 2 after `peabiru <cmd>: <msg>`.
pub fn usage_error(cmd: &str, msg: impl Display) -> ! {
    eprintln!("peabiru {cmd}: {msg}");
    std::process::exit(2)
}

/// Exit with status 1 after `peabiru <cmd>: <msg>`.
pub fn runtime_error(cmd: &str, msg: impl Display) -> ! {
    eprintln!("peabiru {cmd}: {msg}");
    std::process::exit(1)
}

/// The elevation model of the environment variable `PEABIRU_ELEVATION`
/// (`TOBLER`, `MINETTI` or `NONE`, in any case), `None` without it. The
/// command-line tool builds networks without elevation by default, even
/// from a directory with a `.tif` raster (r5r's default is `TOBLER`).
pub fn elevation(cmd: &str) -> Option<peabiru_core::Elevation> {
    let v = std::env::var("PEABIRU_ELEVATION").ok()?;
    Some(
        peabiru_core::Elevation::from_r5r_name(&v).unwrap_or_else(|| {
            usage_error(
                cmd,
                format!("PEABIRU_ELEVATION is '{v}', expected TOBLER, MINETTI or NONE"),
            )
        }),
    )
}

/// Build the network of a data directory, as
/// [`peabiru_core::build_from_dir`]. As r5r, the build fails when a GTFS
/// feed has errors of priority `HIGH`; with the environment variable
/// `PEABIRU_IGNORE_GTFS_ERRORS=1` such feeds are built anyway
/// ([`peabiru_core::BuildOptions::ignore_gtfs_errors`]), with one
/// `peabiru <cmd>: warning: ...` line on stderr per feed and case that r5r
/// refuses to build a network from, and one per warning about the
/// elevation raster. `PEABIRU_ELEVATION` sets the elevation model
/// ([`elevation`]).
pub fn build_from_dir(
    cmd: &str,
    dir: impl AsRef<std::path::Path>,
    opts: &peabiru_core::BuildOptions,
) -> Result<peabiru_core::Network, peabiru_core::NetworkError> {
    let mut opts = opts.clone();
    opts.ignore_gtfs_errors = std::env::var("PEABIRU_IGNORE_GTFS_ERRORS").as_deref() == Ok("1");
    if let Some(e) = elevation(cmd) {
        opts.elevation = e;
    }
    let built = peabiru_core::build_from_dir(dir, &opts)?;
    for report in built.transit().map_or(&[][..], |t| t.feed_reports()) {
        for w in report.r5r_warnings_per_case() {
            eprintln!("peabiru {cmd}: warning: {w}");
        }
    }
    for w in built.street().elevation_warnings() {
        eprintln!("peabiru {cmd}: warning: {w}");
    }
    Ok(built)
}

/// The scenario of the environment variable `PEABIRU_SCENARIO`, a text
/// file of lines `carspeed_scale <x>`, `car scale|km/h`, `way <osm id>
/// <value>` (car speeds by way) and `lts <osm id> <level>` (levels of
/// traffic stress by way), the values written as r5r hands them to R5;
/// the default scenario (no change) without the variable. Polygons and
/// lines are not read here (the packages take them).
pub fn scenario(cmd: &str) -> peabiru_core::Scenario {
    use peabiru_core::unstable::{java_parse_float, java_parse_int, java_parse_long};
    use peabiru_core::{CarSpeeds, LtsChange};
    let mut s = peabiru_core::Scenario::default();
    let Ok(path) = std::env::var("PEABIRU_SCENARIO") else {
        return s;
    };
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| runtime_error(cmd, format!("cannot read {path}: {e}")));
    let (mut car, mut kmh, mut ways, mut lts) = (false, false, Vec::new(), Vec::new());
    for (n, line) in text.lines().enumerate() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let bad = || -> ! { runtime_error(cmd, format!("{path}:{}: cannot read '{line}'", n + 1)) };
        match f.as_slice() {
            [] => {}
            ["carspeed_scale", x] => {
                s.car_speed_scale = java_parse_float(x).unwrap_or_else(|| bad())
            }
            ["car", "scale"] => car = true,
            ["car", "km/h"] => (car, kmh) = (true, true),
            ["way", id, v] => ways.push((
                java_parse_long(id).unwrap_or_else(|| bad()),
                java_parse_float(v).unwrap_or_else(|| bad()),
            )),
            ["lts", id, v] => lts.push((
                java_parse_long(id).unwrap_or_else(|| bad()),
                java_parse_int(v).unwrap_or_else(|| bad()),
            )),
            _ => bad(),
        }
    }
    if car {
        s.car_speeds = if kmh {
            CarSpeeds::KmhByWay(ways)
        } else {
            CarSpeeds::ScaleByWay(ways)
        };
    }
    if !lts.is_empty() {
        s.lts = LtsChange::ByWay(lts);
    }
    s
}

/// Whether the user asked for help anywhere on the command line.
pub fn wants_help(args: &[String]) -> bool {
    args.iter().any(|a| a == "--help" || a == "-h")
}

/// A whole number in `lo..=hi`, read as `i64` and converted with
/// `try_from`, so a value that does not fit the target type is an error
/// instead of wrapping.
pub fn int<T>(cmd: &str, name: &str, v: &str, lo: T, hi: T) -> T
where
    T: TryFrom<i64> + PartialOrd + Display + Copy,
{
    let range = || format!("{name} must be a whole number from {lo} to {hi}, got '{v}'");
    let x: i64 = v
        .trim()
        .parse()
        .unwrap_or_else(|_| usage_error(cmd, range()));
    match T::try_from(x) {
        Ok(t) if t >= lo && t <= hi => t,
        _ => usage_error(cmd, range()),
    }
}

/// Largest value accepted for counts the engine takes as `int` (R's
/// integer range).
pub const INT_MAX: i64 = i32::MAX as i64;

/// A positive count (`1..=i32::MAX`) as `i32`; the engine checks the
/// tighter bounds that depend on other parameters.
pub fn count(cmd: &str, name: &str, v: &str) -> i32 {
    int(cmd, name, v, 1, i32::MAX)
}

/// A non-negative count (`0..=i32::MAX`) as `usize`.
pub fn size(cmd: &str, name: &str, v: &str) -> usize {
    int(cmd, name, v, 0, INT_MAX as usize)
}

/// A finite decimal number.
pub fn float(cmd: &str, name: &str, v: &str) -> f64 {
    match v.trim().parse::<f64>() {
        Ok(x) if x.is_finite() => x,
        _ => usage_error(cmd, format!("{name} must be a finite number, got '{v}'")),
    }
}

/// A comma- (or semicolon-) separated list.
pub fn list<T: FromStr>(cmd: &str, name: &str, v: &str) -> Vec<T> {
    v.split([',', ';'])
        .map(|x| {
            x.trim()
                .parse()
                .unwrap_or_else(|_| usage_error(cmd, format!("{name}: bad value '{x}' in '{v}'")))
        })
        .collect()
}

/// `HH:MM` or `HH:MM:SS` as seconds after midnight (hours may pass 24).
pub fn time_of_day(cmd: &str, name: &str, v: &str) -> i32 {
    let bad = || -> ! { usage_error(cmd, format!("{name} must be HH:MM or HH:MM:SS, got '{v}'")) };
    let parts: Vec<&str> = v.trim().split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        bad();
    }
    let mut n = [0i64; 3];
    for (k, p) in parts.iter().enumerate() {
        if p.is_empty() || p.len() > 6 || !p.bytes().all(|b| b.is_ascii_digit()) {
            bad();
        }
        n[k] = p.parse().unwrap_or_else(|_| bad());
    }
    if n[1] > 59 || n[2] > 59 {
        bad();
    }
    i32::try_from(n[0] * 3600 + n[1] * 60 + n[2]).unwrap_or_else(|_| bad())
}

/// `YYYY-MM-DD` (or `YYYYMMDD`) as the number `yyyymmdd`. Whether it is a
/// real calendar date is the engine's check.
pub fn date(cmd: &str, name: &str, v: &str) -> u32 {
    let bad = || -> ! { usage_error(cmd, format!("{name} must be YYYY-MM-DD, got '{v}'")) };
    let s = v.trim();
    let digits: String = match s.split('-').collect::<Vec<_>>().as_slice() {
        [y, m, d] if y.len() == 4 && m.len() == 2 && d.len() == 2 => format!("{y}{m}{d}"),
        [ymd] if ymd.len() == 8 => ymd.to_string(),
        _ => bad(),
    };
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        bad();
    }
    digits.parse().unwrap_or_else(|_| bad())
}

/// The value after option `args[i]`.
pub fn value<'a>(cmd: &str, args: &'a [String], i: usize) -> &'a str {
    args.get(i + 1)
        .map(String::as_str)
        .unwrap_or_else(|| usage_error(cmd, format!("{} needs a value", args[i])))
}

/// Write `contents` to `path`, exiting with status 1 on failure.
pub fn write_file(cmd: &str, path: &str, contents: impl AsRef<[u8]>) {
    std::fs::write(path, contents)
        .unwrap_or_else(|e| runtime_error(cmd, format!("cannot write {path}: {e}")));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_parse() {
        assert_eq!(time_of_day("t", "--time", "14:00:00"), 50400);
        assert_eq!(time_of_day("t", "--time", "14:05"), 50700);
        assert_eq!(time_of_day("t", "--time", "25:00:01"), 90001);
    }

    #[test]
    fn dates_parse() {
        assert_eq!(date("t", "--date", "2019-05-13"), 20190513);
        assert_eq!(date("t", "--date", "20190513"), 20190513);
    }

    #[test]
    fn integers_in_range() {
        assert_eq!(int::<i32>("t", "--tw", "10", 1, 100), 10);
        assert_eq!(size("t", "--threads", "0"), 0);
        assert_eq!(int("t", "--seed", "-5", i32::MIN, i32::MAX), -5);
    }
}
