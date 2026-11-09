//! Helpers shared by the examples: where the sample data is, and a minimal
//! reader for r5r-style point files (`id,lon,lat,...` with a header).

use std::path::PathBuf;

/// The network directory given as the first argument, or the small Sao Paulo
/// network bundled with the R package (`r/inst/extdata/spo`): one `.pbf`
/// and one frequency-based GTFS `.zip`.
pub fn data_dir() -> PathBuf {
    std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../r/inst/extdata/spo")
        })
}

/// Points of a CSV file with a header: ids, `(lat, lon)` pairs, and the
/// values of the named numeric columns (`columns[k][j]` for point `j`).
pub struct Points {
    pub ids: Vec<String>,
    pub coords: Vec<(f64, f64)>,
    pub columns: Vec<Vec<f64>>,
}

/// Read the first `.csv` in `dir`, keeping the named numeric columns.
pub fn read_points(dir: &std::path::Path, columns: &[&str]) -> Points {
    let csv = std::fs::read_dir(dir)
        .expect("readable data directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.extension().is_some_and(|x| x == "csv"))
        .expect("a .csv file with points in the data directory");
    let text = std::fs::read_to_string(&csv).expect("readable points file");
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .unwrap_or("")
        .split(',')
        .map(|h| h.trim_matches('"'))
        .collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .unwrap_or_else(|| panic!("no '{name}' column in {}", csv.display()))
    };
    let (ci, clon, clat) = (col("id"), col("lon"), col("lat"));
    let cols: Vec<usize> = columns.iter().map(|c| col(c)).collect();
    let mut p = Points {
        ids: Vec::new(),
        coords: Vec::new(),
        columns: vec![Vec::new(); cols.len()],
    };
    for line in lines.filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split(',').map(|x| x.trim_matches('"')).collect();
        let num = |i: usize| f[i].parse::<f64>().unwrap_or(f64::NAN);
        p.ids.push(f[ci].to_string());
        p.coords.push((num(clat), num(clon)));
        for (k, &c) in cols.iter().enumerate() {
            p.columns[k].push(num(c));
        }
    }
    p
}
