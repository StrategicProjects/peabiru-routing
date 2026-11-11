//! The command line on bad input: a one-line message naming the argument,
//! or the file and line, and exit status 2 (usage) or 1 (runtime failure),
//! never a panic.

use std::path::PathBuf;
use std::process::{Command, Output};

const SPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../r/inst/extdata/spo");

fn points() -> String {
    format!("{SPO}/spo_points.csv")
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_peabiru"))
        .args(args)
        .output()
        .expect("run peabiru")
}

/// A scratch file under the target directory.
fn tmp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("peabiru-cli-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// Asserts the exit status and that stderr ends with a single error line
/// (`peabiru <command>: ...`) containing `needle`; `ttm` on a network
/// folder prints a summary of the feeds before it.
#[track_caller]
fn fails(out: &Output, code: i32, needle: &str) {
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(code), "stderr: {err}");
    let errors: Vec<&str> = err.lines().filter(|l| l.starts_with("peabiru ")).collect();
    assert_eq!(errors.len(), 1, "one error line expected: {err}");
    assert_eq!(err.trim_end().lines().last(), Some(errors[0]), "{err}");
    assert!(errors[0].contains(needle), "'{needle}' not in: {err}");
}

fn transit(extra: &[&str]) -> Output {
    let out = tmp("transit.csv");
    let p = points();
    let mut args = vec!["transit", SPO, &p, out.to_str().unwrap()];
    args.extend_from_slice(extra);
    run(&args)
}

const WHEN: [&str; 4] = ["--date", "2019-05-13", "--time", "14:00:00"];

#[test]
fn help_goes_to_stdout_with_status_0() {
    for args in [&["--help"][..], &["-h"], &["transit", "--help"], &["help"]] {
        let out = run(args);
        assert_eq!(out.status.code(), Some(0), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("usage: peabiru"));
        assert!(out.stderr.is_empty());
    }
}

#[test]
fn no_arguments_print_usage_with_status_2() {
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("usage: peabiru"));
}

#[test]
fn unknown_command_and_option() {
    fails(&run(&["frobnicate"]), 2, "unknown command");
    fails(&transit(&["--colour", "red"]), 2, "--colour");
    fails(&transit(&["--tw"]), 2, "--tw needs a value");
    fails(&run(&["transit", SPO]), 2, "expected <network>");
}

#[test]
fn malformed_times_and_dates_name_the_argument() {
    let t = |v: &str| transit(&["--date", "2019-05-13", "--time", v]);
    fails(&t("14"), 2, "--time");
    fails(&t("14:xx"), 2, "--time");
    fails(&t("14:60"), 2, "--time");
    fails(&t("-1:00"), 2, "--time");
    let d = |v: &str| transit(&["--date", v, "--time", "14:00"]);
    fails(&d("2019-5-13"), 2, "--date");
    fails(&d("tomorrow"), 2, "--date");
}

#[test]
fn numbers_are_range_checked_instead_of_wrapping() {
    let with = |name: &str, v: &str| {
        let mut a = WHEN.to_vec();
        a.extend_from_slice(&[name, v]);
        transit(&a)
    };
    fails(&with("--tw", "4294967306"), 2, "--tw");
    fails(&with("--tw", "0"), 2, "--tw");
    fails(&with("--threads", "-1"), 2, "--threads");
    fails(&with("--max-rides", "-3"), 2, "--max-rides");
    fails(&with("--seed", "1e3"), 2, "--seed");
    fails(&with("--percentiles", "50,x"), 2, "--percentiles");
    fails(
        &run(&["ttm", SPO, &points(), "WALK", "o.csv", "-2"]),
        2,
        "n_threads",
    );
    fails(&run(&["osm-stats", "x.pbf", "many"]), 2, "n_threads");
}

#[test]
fn bad_point_files_name_the_file_and_line() {
    let write = |name: &str, body: &str| {
        let p = tmp(name);
        std::fs::write(&p, body).unwrap();
        p.to_str().unwrap().to_string()
    };
    let o = tmp("o.csv");
    let o = o.to_str().unwrap();
    let ttm = |pts: &str| run(&["ttm", SPO, pts, "WALK", o]);
    let bad_coord = write(
        "bad_coord.csv",
        "id,lon,lat\na,-46.63,-23.55\nb,west,-23.55\n",
    );
    fails(&ttm(&bad_coord), 1, "bad_coord.csv:3: lon 'west'");
    let short = write("short.csv", "id,lon,lat\na,-46.63,-23.55\nb,-46.63\n");
    fails(&ttm(&short), 1, "short.csv:3");
    let no_lat = write("no_lat.csv", "id,lon\na,-46.63\n");
    fails(&ttm(&no_lat), 1, "no 'lat' column");
    fails(
        &ttm("/nonexistent/points.csv"),
        1,
        "cannot read /nonexistent/points.csv",
    );

    let acc = |pts: &str| {
        run(&[
            "accessibility",
            SPO,
            pts,
            o,
            "--opportunities",
            "jobs",
            "--cutoffs",
            "30",
        ])
    };
    let acc_short = write(
        "acc_short.csv",
        "id,lon,lat,jobs\na,-46.63,-23.55,1\nb,-46.63\n",
    );
    fails(&acc(&acc_short), 1, "acc_short.csv:3");
    let acc_bad = write("acc_bad.csv", "id,lon,lat,jobs\na,-46.63,-23.55,many\n");
    fails(&acc(&acc_bad), 1, "acc_bad.csv:2");
}

#[test]
fn an_unwritable_output_is_a_runtime_error() {
    let out = run(&["ttm", SPO, &points(), "WALK", "/nonexistent/dir/out.csv"]);
    fails(&out, 1, "cannot write /nonexistent/dir/out.csv");
    let p = points();
    let mut args = vec!["expanded", SPO, &p, "/nonexistent/dir/out.csv"];
    args.extend_from_slice(&WHEN);
    fails(&run(&args), 1, "cannot create /nonexistent/dir/out.csv");
}

#[test]
fn bad_mode_and_rejected_parameters_are_usage_errors() {
    fails(&run(&["ttm", SPO, &points(), "BOAT", "o.csv"]), 2, "BOAT");
    let mut a = WHEN.to_vec();
    a.extend_from_slice(&["--percentiles", "100"]);
    fails(&transit(&a), 2, "percentiles");
    let (p, o) = (points(), tmp("o.csv"));
    let o = o.to_str().unwrap();
    for lts in ["0", "7"] {
        let out = run(&[
            "accessibility",
            SPO,
            &p,
            o,
            "--opportunities",
            "jobs",
            "--cutoffs",
            "30",
            "--mode",
            "BICYCLE",
            "--max-lts",
            lts,
        ]);
        fails(&out, 2, "max_lts must be from 1 to 4");
    }
}

#[test]
fn transit_commands_need_a_real_date() {
    fails(
        &transit(&["--time", "14:00"]),
        2,
        "--date and --time are required",
    );
    fails(
        &transit(&["--date", "2019-05-13"]),
        2,
        "--date and --time are required",
    );
    let bad = ["--date", "2019-02-30", "--time", "14:00"];
    fails(&transit(&bad), 2, "date must be a calendar date");
    let o = tmp("o.csv");
    let (p, o) = (points(), o.to_str().unwrap().to_string());
    let mut args = vec!["expanded", SPO, &p, &o];
    args.extend_from_slice(&bad);
    fails(&run(&args), 2, "date must be a calendar date");
    args.push("--arrival");
    fails(&run(&args), 2, "date must be a calendar date");
    let acc = [
        "accessibility",
        SPO,
        &p,
        &o,
        "--opportunities",
        "jobs",
        "--cutoffs",
        "30",
    ];
    let mut args = acc.to_vec();
    args.extend_from_slice(&["--mode", "WALK,TRANSIT"]);
    fails(&run(&args), 2, "--date and --time are required");
    args.extend_from_slice(&bad);
    fails(&run(&args), 2, "date must be a calendar date");
}

/// r5r 2.5.0 answers a 2000-minute window and 200 rides; so does peabiru.
/// What stays bounded is the int clock, with a message naming the
/// parameters the user set.
#[test]
fn only_r5_bounds_and_the_int_clock_remain() {
    let with = |extra: &[&str]| {
        let mut a = WHEN.to_vec();
        a.extend_from_slice(extra);
        transit(&a)
    };
    for ok in [&["--tw", "2000"][..], &["--max-rides", "200"]] {
        let out = with(ok);
        assert_eq!(out.status.code(), Some(0), "{ok:?}: {out:?}");
    }
    fails(
        &with(&["--max-trip", "2147483647"]),
        2,
        "max_trip_duration must be from 1 to 35791394 minutes",
    );
    fails(
        &with(&["--tw", "30000000", "--max-trip", "10000000"]),
        2,
        "reduce time_window or max_trip_duration",
    );
}

/// A zip archive of stored entries (the reader does not check CRCs).
fn stored_zip(files: &[(&str, &str)]) -> Vec<u8> {
    let (mut out, mut cd) = (Vec::new(), Vec::new());
    for (name, data) in files {
        let offset = out.len() as u32;
        let sizes = [(data.len() as u32).to_le_bytes(); 2].concat();
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend([20, 0, 0, 0, 0, 0]); // version, flags, method 0
        out.extend([0; 8]); // time, date, crc
        out.extend(&sizes);
        out.extend((name.len() as u16).to_le_bytes());
        out.extend([0, 0]);
        out.extend(name.as_bytes());
        out.extend(data.as_bytes());
        cd.extend(0x0201_4b50u32.to_le_bytes());
        cd.extend([20, 0, 20, 0, 0, 0, 0, 0]);
        cd.extend([0; 8]);
        cd.extend(&sizes);
        cd.extend((name.len() as u16).to_le_bytes());
        cd.extend([0; 12]); // extra, comment, disk, attributes
        cd.extend(offset.to_le_bytes());
        cd.extend(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    out.extend(&cd);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0; 4]);
    out.extend([(files.len() as u16).to_le_bytes(); 2].concat());
    out.extend((cd.len() as u32).to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend([0, 0]);
    out
}

/// A feed that r5r refuses to build (here: no agency.txt) is refused; with
/// `PEABIRU_IGNORE_GTFS_ERRORS=1` it is built, with one warning line naming
/// the file; the sample feed gives none.
#[test]
fn feeds_r5r_refuses_are_refused_or_warned_about() {
    let dir = tmp("no-agency-net");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(format!("{SPO}/spo_osm.pbf"), dir.join("spo_osm.pbf")).unwrap();
    let feed = stored_zip(&[
        (
            "stops.txt",
            "stop_id,stop_lat,stop_lon\nA,-23.5563,-46.6290\nB,-23.5577,-46.6321\n",
        ),
        ("routes.txt", "route_id,route_type\nR,3\n"),
        ("trips.txt", "route_id,service_id,trip_id\nR,S,T1\n"),
        (
            "stop_times.txt",
            "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
T1,14:00:00,14:00:00,A,1\nT1,14:05:00,14:05:00,B,2\n",
        ),
        (
            "calendar.txt",
            "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,\
start_date,end_date\nS,1,1,1,1,1,1,1,20190101,20191231\n",
        ),
    ]);
    std::fs::write(dir.join("bus.zip"), feed).unwrap();
    let o = tmp("no-agency.csv");
    let p = points();
    let mut args = vec!["transit", dir.to_str().unwrap(), &p, o.to_str().unwrap()];
    args.extend_from_slice(&WHEN);
    let out = run(&args);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "stderr: {err}");
    assert_eq!(
        err.trim_end(),
        format!(
            "peabiru transit: high priority GTFS errors found in {} (1 error); network build \
             failed (as in r5r)",
            dir.join("bus.zip").display()
        )
    );
    let out = Command::new(env!("CARGO_BIN_EXE_peabiru"))
        .args(&args)
        .env("PEABIRU_IGNORE_GTFS_ERRORS", "1")
        .output()
        .expect("run peabiru");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr: {err}");
    let warnings: Vec<&str> = err.lines().filter(|l| l.contains("warning")).collect();
    assert_eq!(
        warnings,
        [
            "peabiru transit: warning: GTFS feed 'bus.zip': it has no agency.txt. \
r5r refuses to build a network from this feed."
        ],
        "{err}"
    );

    let out = transit(&WHEN);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "stderr: {err}");
    assert!(!err.contains("warning"), "{err}");
}
