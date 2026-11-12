//! The commands of `peabiru` on the São Paulo sample of the repository:
//! what they write, their exit status on bad input (2 an argument, 1 a
//! file), and that bad input never panics. The values themselves are
//! compared with the R package by bench/compare/cli_parity.R.

use std::path::PathBuf;
use std::process::{Command, Output};

const SPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../r/inst/extdata/spo");
const DEP: [&str; 2] = ["--departure", "2019-05-13T14:00"];

fn points() -> String {
    format!("{SPO}/spo_points.csv")
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_peabiru"))
        .args(args)
        .output()
        .expect("run peabiru")
}

fn tmp(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("peabiru-cli-commands");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[track_caller]
fn ok(o: &Output) -> String {
    assert_eq!(
        o.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    stdout(o)
}

/// Exit status `code`, a message containing `needle`, and no panic.
#[track_caller]
fn fails(o: &Output, code: i32, needle: &str) {
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(code), "stderr: {err}");
    assert!(err.contains(needle), "'{needle}' not in: {err}");
    assert!(!err.contains("panicked"), "{err}");
}

#[test]
fn help_and_version() {
    let h = ok(&run(&["--help"]));
    for c in [
        "info", "matrix", "expanded", "arrival", "accessibility", "isochrone", "itineraries",
        "pareto", "options", "snap", "network", "fares", "completions",
    ] {
        assert!(h.contains(&format!("\n  {c} ")), "{c} not in the help");
    }
    // the developer commands stay out of it
    assert!(!h.contains("dump-edges") && !h.contains("ttm"));
    let m = ok(&run(&["matrix", "--help"]));
    assert!(m.contains("--max-walk-time") && m.contains("Examples:"));
    assert_eq!(
        ok(&run(&["--version"])).trim(),
        format!("peabiru {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(ok(&run(&["completions", "bash"])).contains("peabiru"));
}

#[test]
fn street_and_transit_matrices() {
    let p = points();
    let walk = ok(&run(&["matrix", SPO, "--origins", &p, "--mode", "WALK", "-q"]));
    let lines: Vec<&str> = walk.lines().collect();
    assert_eq!(lines[0], "from_id,to_id,travel_time_p50");
    assert_eq!(lines.len(), 1 + 27 * 27);
    let mut args = vec!["matrix", SPO, "--origins", &p, "--mode", "walk,transit", "--seed", "42"];
    args.extend(DEP);
    args.extend(["--percentiles", "75,25", "--quiet"]);
    let t = ok(&run(&args));
    assert!(t.starts_with("from_id,to_id,travel_time_p25,travel_time_p75\n"));
    // the same into a file
    let f = tmp("ttm.csv");
    args.extend(["-o", f.to_str().unwrap()]);
    ok(&run(&args));
    assert_eq!(std::fs::read_to_string(&f).unwrap(), t);
}

#[test]
fn other_tables() {
    let p = points();
    let mut a = vec!["accessibility", SPO, "--origins", &p, "--opportunities", "jobs"];
    a.extend(["--cutoffs", "30", "-q"]);
    let acc = ok(&run(&a));
    assert!(acc.starts_with("id,opportunity,percentile,cutoff,accessibility\n"));
    assert_eq!(acc.lines().count(), 28);
    let mut e = vec!["expanded", SPO, "--origins", &p, "--mode", "WALK,TRANSIT", "-q"];
    e.extend(DEP);
    e.extend(["--seed", "1", "--time-window", "2", "--breakdown"]);
    assert!(ok(&run(&e)).starts_with("from_id,to_id,departure_time,draw_number,access_time"));
    let snap = ok(&run(&["snap", SPO, "--points", &p, "-q"]));
    assert!(snap.starts_with("point_id,lat,lon,snap_lat,snap_lon,distance,found\n"));
    let it = ok(&run(&["itineraries", SPO, "--origins", &p, "--mode", "CAR", "-q"]));
    assert!(it.lines().next().unwrap().ends_with(",route,geometry"));
}

#[test]
fn geometries_and_the_network() {
    let p = points();
    let mut a = vec!["isochrone", SPO, "--origins", &p, "--mode", "WALK", "-q"];
    a.extend(["--cutoffs", "10,20"]);
    let g = ok(&run(&a));
    assert!(g.starts_with("{\"type\":\"FeatureCollection\",\"features\":["));
    assert!(g.contains("\"MultiPolygon\"") && g.contains("\"percentile\":\"p50\""));
    let b = ok(&run(&["network", SPO, "--layer", "bbox", "--format", "csv", "-q"]));
    assert!(b.starts_with("xmin,ymin,xmax,ymax\n-46.6"));
    let r = ok(&run(&["network", SPO, "--layer", "routes", "-q"]));
    assert!(r.contains("\"LineString\""));
    let i = ok(&run(&["info", SPO, "-q"]));
    assert!(i.contains("feeds: spo_gtfs") && i.contains("frequency-based trips: yes"));
    let e = ok(&run(&["info", SPO, "--gtfs-errors", "-q"]));
    assert!(e.starts_with("file,line,type,field,id,priority\n"));
    let d = ok(&run(&["info", SPO, "--dates", "2019-05-13,2019-05-19", "-q"]));
    assert_eq!(d.lines().count(), 3);
}

#[test]
fn fare_structures_round_trip() {
    let z = tmp("fares.zip");
    let z = z.to_str().unwrap();
    ok(&run(&["fares", "setup", SPO, "--base-fare", "4.4", "-o", z, "-q"]));
    let s = ok(&run(&["fares", "read", z]));
    assert!(s.contains("fare types: "), "{s}");
    let dir = tmp("fares-dir");
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_str().unwrap();
    ok(&run(&["fares", "read", z, "--output-dir", d]));
    let z2 = tmp("fares2.zip");
    ok(&run(&["fares", "write", d, "-o", z2.to_str().unwrap()]));
    assert_eq!(std::fs::read(z).unwrap(), std::fs::read(&z2).unwrap());
}

#[test]
fn bad_input_has_the_right_status() {
    let p = points();
    // arguments
    fails(&run(&["frobnicate"]), 2, "unrecognized subcommand");
    fails(&run(&["matrix", SPO, "--origins", &p, "--mode", "BOAT"]), 2, "BOAT");
    fails(
        &run(&["matrix", SPO, "--origins", &p, "--mode", "WALK,TRANSIT"]),
        2,
        "--departure is required",
    );
    let bad_date = ["matrix", SPO, "--origins", &p, "--mode", "TRANSIT", "--departure", "2019-02-30T14:00"];
    fails(&run(&bad_date), 2, "--departure");
    fails(
        &run(&["matrix", SPO, "--origins", &p, "--percentiles", "100"]),
        2,
        "--percentiles",
    );
    fails(&run(&["matrix", SPO, "--origins", &p, "--max-lts", "7"]), 2, "max-lts");
    fails(
        &run(&["matrix", SPO, "--origins", &p, "--max-fare", "5"]),
        2,
        "--fare-structure",
    );
    // files
    fails(&run(&["matrix", "/nonexistent", "--origins", &p]), 1, "/nonexistent");
    fails(&run(&["matrix", SPO, "--origins", "/nonexistent.csv"]), 1, "cannot read");
    let bad = tmp("bad.csv");
    std::fs::write(&bad, "id,lon,lat\na,west,-23.55\n").unwrap();
    fails(
        &run(&["matrix", SPO, "--origins", bad.to_str().unwrap()]),
        1,
        "bad.csv:2: lon 'west' is not a number",
    );
    let dup = tmp("dup.csv");
    std::fs::write(&dup, "id,lon,lat\na,-46.63,-23.55\na,-46.63,-23.55\n").unwrap();
    fails(&run(&["matrix", SPO, "--origins", dup.to_str().unwrap()]), 1, "duplicated ids");
    fails(
        &run(&["matrix", SPO, "--origins", &p, "-o", "/nonexistent/dir/out.csv"]),
        1,
        "cannot write /nonexistent/dir/out.csv",
    );
}

#[test]
fn quoted_ids_are_read_and_written_as_rfc_4180() {
    let f = tmp("quoted.csv");
    std::fs::write(
        &f,
        "\"id\",lon,lat\n\"a, \"\"first\"\"\",-46.6290,-23.5563\nb,-46.6321,-23.5577\n",
    )
    .unwrap();
    let out = ok(&run(&["matrix", SPO, "--origins", f.to_str().unwrap(), "-q"]));
    assert!(out.contains("\"a, \"\"first\"\"\",b,"), "{out}");
}

#[test]
fn old_spellings_still_run() {
    let o = tmp("old.csv");
    let (p, o) = (points(), o.to_str().unwrap().to_string());
    let out = run(&["accessibility", SPO, &p, &o, "--opportunities", "jobs", "--cutoffs", "30"]);
    ok(&out);
    assert!(std::fs::read_to_string(&o).unwrap().starts_with("id,opportunity"));
    ok(&run(&["dev", "ttm", SPO, &p, "WALK", &o]));
}
