//! CSV output: matrices routed from slices of the origins write the files
//! of the whole matrix, byte for byte.

#[path = "../src/sample_data.rs"]
mod sample_data;

use peabiru_core::{
    build_from_dir, expanded_travel_time_matrix, street_accessibility, street_travel_time_matrix,
    transit_accessibility, transit_travel_time_matrix, AccessibilityParams, BuildOptions,
    CsvOutput, DecayFunction, Destinations, Mode, RouteParams, TransitParams,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The fixture's points as `(id, (lat, lon))`.
fn points(spo: &str) -> (Vec<String>, Vec<(f64, f64)>) {
    let text = std::fs::read_to_string(format!("{spo}/spo_points.csv")).unwrap();
    text.lines()
        .skip(1)
        .map(|line| {
            let f: Vec<&str> = line.split(',').collect();
            (
                f[0].to_string(),
                (f[2].parse().unwrap(), f[1].parse().unwrap()),
            )
        })
        .unzip()
}

/// A new empty directory under the system's temporary directory.
fn empty_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("peabiru-output-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn contents(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().into_string().unwrap(),
                std::fs::read(e.path()).unwrap(),
            )
        })
        .collect()
}

/// Write with `write(writer, origins)` once for all the origins and once a
/// few origins at a time; both directories must hold the same files.
fn same_in_slices(
    name: &str,
    ids: &[String],
    dest_ids: &[String],
    pts: &[(f64, f64)],
    write: impl Fn(&CsvOutput, usize, &[(f64, f64)]),
) -> BTreeMap<String, Vec<u8>> {
    let whole = empty_dir(&format!("{name}-whole"));
    let w = CsvOutput::new(whole.to_str().unwrap(), ids, dest_ids, 2).unwrap();
    write(&w, 0, pts);
    let sliced = empty_dir(&format!("{name}-sliced"));
    let w = CsvOutput::new(sliced.to_str().unwrap(), ids, dest_ids, 3).unwrap();
    let mut start = 0;
    for size in [1, 4, 2, 7].iter().cycle() {
        if start >= pts.len() {
            break;
        }
        let end = (start + size).min(pts.len());
        write(&w, start, &pts[start..end]);
        start = end;
    }
    let (a, b) = (contents(&whole), contents(&sliced));
    assert!(!a.is_empty(), "{name}: no file written");
    assert_eq!(a, b, "{name}");
    let _ = std::fs::remove_dir_all(&whole);
    let _ = std::fs::remove_dir_all(&sliced);
    a
}

#[test]
fn slices_of_the_origins_write_the_files_of_the_whole() {
    let Some(spo) = sample_data::spo() else {
        return;
    };
    let net = build_from_dir(spo, &BuildOptions::default()).unwrap();
    let (ids, pts) = points(spo);

    let p = RouteParams::default();
    let files = same_in_slices("walk", &ids, &ids, &pts, |w, first, o| {
        let m = street_travel_time_matrix(&net, o, &pts, Mode::Walk, &p).unwrap();
        w.write_street_matrix(first, &m, &[25, 75]).unwrap();
    });
    // one file per origin that reaches a destination, named after its id
    let first = std::str::from_utf8(&files[&format!("from_{}.csv", ids[0])]).unwrap();
    // lines end as Java's println ends them
    let eol = if cfg!(windows) { "\r\n" } else { "\n" };
    assert!(first.ends_with(eol) && !first.replace(eol, "").contains(['\r', '\n']));
    let first = first.replace(eol, "\n");
    assert!(first.starts_with("from_id,to_id,travel_time_p25,travel_time_p75\n"));
    assert!(first.contains(&format!("\n{},{},0,0\n", ids[0], ids[0])));

    let mut t = TransitParams::default();
    t.date = 20190513;
    t.departure_time = 14 * 3600;
    t.percentiles = vec![5, 50, 95];
    t.max_trip_duration = 40;
    t.seed = Some(42);
    t.n_threads = 2;
    same_in_slices("transit", &ids, &ids, &pts, |w, first, o| {
        let m = transit_travel_time_matrix(&net, o, &pts, &t).unwrap();
        w.write_transit_matrix(first, &m).unwrap();
    });

    let mut e = t.clone();
    e.time_window = 3;
    for breakdown in [false, true] {
        same_in_slices(
            &format!("expanded-{breakdown}"),
            &ids,
            &ids,
            &pts,
            |w, first, o| {
                let m = expanded_travel_time_matrix(&net, o, &pts, &e).unwrap();
                w.write_expanded(first, &m, breakdown).unwrap();
            },
        );
    }

    let jobs: Vec<f64> = (0..pts.len()).map(|j| (j * 37 % 11) as f64).collect();
    let opp = vec![jobs.clone(), jobs.iter().map(|v| v * 0.5).collect()];
    let dest = Destinations::new(&pts, &opp);
    let names = vec!["jobs".to_string(), "half".to_string()];
    let mut a = AccessibilityParams::default();
    a.cutoffs = vec![10, 30];
    a.decay = DecayFunction::Logistic {
        standard_deviation_minutes: 5.0,
    };
    same_in_slices("access-transit", &ids, &[], &pts, |w, first, o| {
        let acc = transit_accessibility(&net, o, &dest, &t, &a).unwrap();
        w.write_accessibility(first, &acc, &names).unwrap();
    });
    let files = same_in_slices("access-car", &ids, &[], &pts, |w, first, o| {
        let acc = street_accessibility(&net, o, &dest, Mode::Car, &p, &[50], &a).unwrap();
        w.write_accessibility(first, &acc, &names).unwrap();
    });
    // a file for every origin, reached or not
    assert_eq!(files.len(), ids.len());
}

#[test]
fn an_unwritable_file_is_an_error_naming_it() {
    let Some(spo) = sample_data::spo() else {
        return;
    };
    let net = build_from_dir(spo, &BuildOptions::default()).unwrap();
    let (_, pts) = points(spo);
    let ids: Vec<String> = (0..pts.len()).map(|i| format!("p{i}")).collect();
    let dir = empty_dir("unwritable");
    // a directory that is not there (ids naming one are refused, see the
    // unit tests of CsvOutput::new)
    let gone = dir.join("no").join("such");
    let w = CsvOutput::new(gone.to_str().unwrap(), &ids, &ids, 2).unwrap();
    let m =
        street_travel_time_matrix(&net, &pts, &pts, Mode::Walk, &RouteParams::default()).unwrap();
    let e = w.write_street_matrix(0, &m, &[50]).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
    assert!(e.to_string().contains("from_p"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}
