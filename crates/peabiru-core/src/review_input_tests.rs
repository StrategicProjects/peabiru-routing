//! Reproductions for the pre-release review of the input readers and the
//! network build (OSM PBF, GTFS zip/CSV, street build, stop linking).
//! Each test documents current behaviour; see the review report.

use crate::gtfs::zip::Archive;
use crate::osm::pbf::{read_pbf, Extract, ReadOptions, Tags, Way};
use crate::street::build::network_from_extract;
use crate::transit::TransitLayer;
use std::time::Instant;

// ---------------------------------------------------------------- encoders

fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}
fn zz(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}
fn fvar(out: &mut Vec<u8>, field: u32, v: u64) {
    varint(out, u64::from(field) << 3);
    varint(out, v);
}
fn fbytes(out: &mut Vec<u8>, field: u32, b: &[u8]) {
    varint(out, (u64::from(field) << 3) | 2);
    varint(out, b.len() as u64);
    out.extend_from_slice(b);
}
fn packed(vals: impl IntoIterator<Item = u64>) -> Vec<u8> {
    let mut o = Vec::new();
    for v in vals {
        varint(&mut o, v);
    }
    o
}
fn packed_delta(vals: &[i64]) -> Vec<u8> {
    let mut prev = 0;
    packed(vals.iter().map(|&v| {
        let d = v - prev;
        prev = v;
        zz(d)
    }))
}
/// One raw (uncompressed) blob with an explicit `datasize`.
fn blob_with_datasize(kind: &str, payload: &[u8], datasize: u64) -> Vec<u8> {
    let mut b = Vec::new();
    fbytes(&mut b, 1, payload);
    let mut header = Vec::new();
    fbytes(&mut header, 1, kind.as_bytes());
    fvar(&mut header, 3, datasize);
    let mut out = (header.len() as u32).to_be_bytes().to_vec();
    out.extend(header);
    out.extend(b);
    out
}
fn blob(kind: &str, payload: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    fbytes(&mut b, 1, payload);
    blob_with_datasize(kind, payload, b.len() as u64)
}

fn all(_: &[(&str, &str)]) -> bool {
    true
}
fn opts() -> ReadOptions<'static> {
    ReadOptions {
        way_filter: &all,
        relation_filter: None,
        node_tag_filter: None,
        poi_filter: None,
        ref_count_filter: None,
        n_threads: 1,
    }
}

// ---------------------------------------------------------------- PBF

/// An empty (0-byte) file is accepted as an empty extract; building gives
/// an empty network without an error. A truncated download therefore
/// surfaces much later (every point unlinked) instead of at build time.
#[test]
fn pbf_empty_file_is_silently_empty() {
    let ex = read_pbf(b"", &opts()).unwrap();
    assert!(ex.ways.is_empty() && ex.node_ids.is_empty());
    // also: a file holding only an OSMHeader blob
    let f = blob("OSMHeader", b"");
    assert!(read_pbf(&f, &opts()).unwrap().ways.is_empty());
}

/// One block with dense nodes 1..=4 on a line and way 10 over them.
fn line_block() -> Vec<u8> {
    let mut st = Vec::new();
    for s in ["", "highway", "residential"] {
        fbytes(&mut st, 1, s.as_bytes());
    }
    let mut dense = Vec::new();
    fbytes(&mut dense, 1, &packed_delta(&[1, 2, 3, 4]));
    fbytes(
        &mut dense,
        8,
        &packed_delta(&[-235_000_000, -235_010_000, -235_020_000, -235_030_000]),
    );
    fbytes(&mut dense, 9, &packed_delta(&[-466_000_000; 4]));
    let mut way = Vec::new();
    fvar(&mut way, 1, 10);
    fbytes(&mut way, 2, &packed([1]));
    fbytes(&mut way, 3, &packed([2]));
    fbytes(&mut way, 8, &packed_delta(&[1, 2, 3, 4]));
    let (mut g1, mut g2) = (Vec::new(), Vec::new());
    fbytes(&mut g1, 2, &dense);
    fbytes(&mut g2, 3, &way);
    let mut block = Vec::new();
    fbytes(&mut block, 1, &st);
    fbytes(&mut block, 2, &g1);
    fbytes(&mut block, 2, &g2);
    block
}

/// The same way present twice (e.g. extracts concatenated with
/// `osmium cat`, which does not deduplicate) is kept twice: two identical
/// edge pairs, and every node's reference count doubles, so every node of
/// the way becomes an "intersection" and the way is cut at every node.
/// osm-lib (R5) stores ways in a map by id, so the duplicate collapses.
#[test]
fn pbf_duplicate_way_is_kept_twice() {
    let ref_all = |_: &[(&str, &str)]| true;
    let o = ReadOptions {
        ref_count_filter: Some(&ref_all),
        ..opts()
    };
    let once = blob("OSMData", &line_block());
    let mut twice = once.clone();
    twice.extend(blob("OSMData", &line_block()));
    let a = read_pbf(&once, &o).unwrap();
    let b = read_pbf(&twice, &o).unwrap();
    eprintln!(
        "pbf_duplicate_way: once: {} ways, ref_counts {:?}; twice: {} ways, ref_counts {:?}",
        a.ways.len(),
        a.ref_counts,
        b.ways.len(),
        b.ref_counts
    );
    let na = network_from_extract(a);
    let nb = network_from_extract(b);
    eprintln!(
        "pbf_duplicate_way: edge pairs once = {}, twice = {}",
        na.pair_from.len(),
        nb.pair_from.len()
    );
    assert_eq!(na.pair_from.len(), 1);
    assert_eq!(nb.pair_from.len(), 6);
}

/// A 0-byte .pbf next to a real GTFS feed builds "successfully": no streets,
/// every stop unlinked, no error or warning.
#[test]
fn build_from_dir_with_empty_pbf() {
    let dir = std::env::temp_dir().join(format!("peabiru-review-empty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("empty.osm.pbf"), b"").unwrap();
    let gtfs = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../r/inst/extdata/spo/spo_gtfs.zip"
    );
    std::fs::copy(gtfs, dir.join("spo_gtfs.zip")).unwrap();
    let r = crate::transit::build_from_dir(&dir, &Default::default());
    std::fs::remove_dir_all(&dir).unwrap();
    let (net, layer) = r.unwrap();
    let linked = layer.stop_vertex.iter().filter(|&&v| v >= 0).count();
    eprintln!(
        "build_from_dir_with_empty_pbf: {} edges, {} stops, {} linked",
        net.n_edges(),
        layer.stop_ids.len(),
        linked
    );
    assert_eq!(linked, 0);
}

// ---------------------------------------------------------------- GTFS

fn make_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut cd = Vec::new();
    for (name, data) in files {
        let offset = out.len() as u32;
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend([20, 0, 0, 0, 0, 0]);
        out.extend([0; 8]);
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(name.as_bytes());
        out.extend(*data);
        cd.extend(0x0201_4b50u32.to_le_bytes());
        cd.extend([20, 0, 20, 0, 0, 0, 0, 0]);
        cd.extend([0; 8]);
        cd.extend((data.len() as u32).to_le_bytes());
        cd.extend((data.len() as u32).to_le_bytes());
        cd.extend((name.len() as u16).to_le_bytes());
        cd.extend([0; 12]);
        cd.extend(offset.to_le_bytes());
        cd.extend(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    out.extend(&cd);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((cd.len() as u32).to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend([0, 0]);
    out
}

const ROUTES: &[u8] = b"route_id,route_type\nR,3\n";
const CAL: &[u8] = b"service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nS,1,1,1,1,1,1,1,20190101,20191231\n";

/// A ZIP64 entry (sizes 0xFFFFFFFF in the central directory, real sizes in
/// the extra field; written e.g. by streaming writers) is reported as an
/// entry too large for the limit, not as unsupported ZIP64.
#[test]
fn zip64_entry_message() {
    let mut z = make_zip(&[("stops.txt", b"stop_id\nA\n")]);
    let cd = z
        .windows(4)
        .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .unwrap();
    z[cd + 20..cd + 28].copy_from_slice(&[0xff; 8]);
    let a = Archive::from_bytes(z).unwrap();
    let e = a.read(&a.entries[0]).unwrap_err();
    eprintln!("zip64_entry_message: {e}");
    assert!(e.to_string().contains("exceeds the limit"));
}

/// Reads every entry of the zip named by `PEABIRU_REVIEW_ZIP` (manual check
/// of archives written by other tools).
#[test]
#[ignore]
fn zip_from_env() {
    let Ok(p) = std::env::var("PEABIRU_REVIEW_ZIP") else {
        return;
    };
    let a = Archive::open(&p).unwrap();
    for e in &a.entries {
        eprintln!(
            "zip_from_env {p}: {} -> {:?}",
            e.name,
            a.read(e).map(|b| b.len())
        );
    }
}

/// gtfs-lib looks for the table at the archive root first and only then in
/// subfolders; the reader takes the first entry with that base name in
/// directory order, so a stray `old/stops.txt` listed before the root
/// `stops.txt` wins.
#[test]
fn zip_root_table_not_preferred() {
    let z = make_zip(&[
        ("old/stops.txt", b"stop_id\nOLD\n"),
        ("stops.txt", b"stop_id\nNEW\n"),
    ]);
    let a = Archive::from_bytes(z).unwrap();
    assert_eq!(a.find("stops.txt").unwrap().name, "old/stops.txt");
}

// ---------------------------------------------------------------- network

/// A square grid of residential streets with `n` x `n` nodes, `step` 1e-7
/// degrees apart, starting at (`lat0`, `lon0`) 1e-7 degrees. Node ids start
/// at `id0`, way ids at `id0`.
fn grid(ex: &mut Extract, n: usize, step: i32, lat0: i32, lon0: i32, id0: i64) {
    let id = |r: usize, c: usize| id0 + (r * n + c) as i64;
    let tags = || Tags(vec![("highway".into(), "residential".into())]);
    for r in 0..n {
        ex.ways.push(Way {
            id: id0 + r as i64,
            tags: tags(),
            nodes: (0..n).map(|c| id(r, c)).collect(),
        });
        ex.ways.push(Way {
            id: id0 + (n + r) as i64,
            tags: tags(),
            nodes: (0..n).map(|c| id(c, r)).collect(),
        });
    }
    for r in 0..n {
        for c in 0..n {
            ex.node_ids.push(id(r, c));
            ex.lat.push(lat0 + r as i32 * step);
            ex.lon.push(lon0 + c as i32 * step);
            ex.ref_counts.push(2);
        }
    }
}

/// Two small street grids far apart (a city plus a stray far-away way, or a
/// country-sized extract): the uniform 0.005-degree EdgeIndex grid is sized
/// by the bounding box, so memory grows with the area, not the network.
/// Run with `/usr/bin/time -l` to see the peak RSS.
#[test]
#[ignore]
fn edge_index_grows_with_bbox_area() {
    let mut ex = Extract::default();
    grid(&mut ex, 8, 10_000, -300_000_000, -510_000_000, 1_000);
    grid(&mut ex, 8, 10_000, 300_000_000, 500_000_000, 1_000_000);
    let t = Instant::now();
    let net = network_from_extract(ex);
    eprintln!(
        "edge_index_grows_with_bbox_area: {} pairs, built in {:?}",
        net.pair_from.len(),
        t.elapsed()
    );
}

/// Stop linking splits edges; each split pair goes to EdgeIndex::extra,
/// which every later query returns in full. Linking N stops is O(N^2) and,
/// after linking, every find_split (each origin/destination of every
/// matrix) scans all split pairs.
#[test]
#[ignore]
fn stop_linking_is_quadratic() {
    for n_stops in [0usize, 2_000, 4_000, 8_000, 16_000] {
        let mut ex = Extract::default();
        // 120 x 120 nodes, ~111 m apart: ~13 km square.
        grid(&mut ex, 120, 10_000, -235_000_000, -466_000_000, 1);
        let mut net = network_from_extract(ex);
        let mut layer = TransitLayer::default();
        let side = (n_stops as f64).sqrt().ceil() as usize;
        for i in 0..n_stops {
            let (r, c) = (i / side, i % side);
            // between streets, so every stop splits an edge
            let lat = -23.5 + (r as f64 + 0.37) * (1.19 / side as f64) * 0.1;
            let lon = -46.6 + (c as f64 + 0.41) * (1.19 / side as f64) * 0.1;
            layer.stop_ids.push(format!("f:{i}"));
            layer.stop_lat.push(lat);
            layer.stop_lon.push(lon);
            layer.stop_vertex.push(-1);
        }
        let t = Instant::now();
        layer.link_stops(&mut net);
        let link = t.elapsed();
        let mut scratch = Vec::new();
        let t = Instant::now();
        for i in 0..1000 {
            let _ = crate::street::link::find_split(
                &net,
                -23.5 + 0.0001 * i as f64,
                -46.6 + 0.0001 * i as f64,
                crate::street::Mode::Walk,
                &mut scratch,
            );
        }
        eprintln!(
            "stop_linking_is_quadratic: {n_stops} stops linked in {link:?}; 1000 find_split after linking: {:?}",
            t.elapsed()
        );
    }
}

/// A `type=restriction` relation with many `via` ways that share nodes:
/// the via-way path search enumerates every ordering, so time and memory
/// grow factorially with the number of via ways (one vandalised relation
/// can stall the build of a whole region).
#[test]
#[ignore]
fn restriction_via_ways_blow_up() {
    use crate::osm::pbf::{Member, MemberKind, Relation};
    for k in [5usize, 6, 7, 8] {
        let mut ex = Extract::default();
        grid(&mut ex, 8, 10_000, -235_000_000, -466_000_000, 1);
        // k via ways, all from node 1 to node 2 (the two first grid nodes)
        let tags = || Tags(vec![("highway".into(), "residential".into())]);
        let mut members = vec![Member {
            kind: MemberKind::Way,
            id: 1,
            role: "from".into(),
        }];
        for i in 0..k {
            let id = 10_000 + i as i64;
            ex.ways.push(Way {
                id,
                tags: tags(),
                nodes: vec![1, 2],
            });
            members.push(Member {
                kind: MemberKind::Way,
                id,
                role: "via".into(),
            });
            ex.member_ways_present.push(id);
        }
        members.push(Member {
            kind: MemberKind::Way,
            id: 9,
            role: "to".into(),
        });
        ex.member_ways_present.extend([1, 9]);
        ex.member_ways_present.sort_unstable();
        ex.relations.push(Relation {
            id: 1,
            tags: Tags(vec![
                ("type".into(), "restriction".into()),
                ("restriction".into(), "no_left_turn".into()),
            ]),
            members,
        });
        let t = Instant::now();
        let _ = network_from_extract(ex);
        eprintln!(
            "restriction_via_ways_blow_up: {k} via ways: {:?}",
            t.elapsed()
        );
    }
}

/// Peak memory of loading a large synthetic feed (run with /usr/bin/time -l).
#[test]
#[ignore]
fn gtfs_memory_per_stop_time() {
    let n_trips = 100_000usize;
    let per_trip = 40usize;
    let mut stops = String::from("stop_id,stop_lat,stop_lon\n");
    for s in 0..2_000 {
        stops += &format!("stop_{s:05},{},{}\n", -23.5 + s as f64 * 1e-4, -46.6);
    }
    let mut st = String::from("trip_id,arrival_time,departure_time,stop_id,stop_sequence\n");
    let mut trips = String::from("route_id,service_id,trip_id\n");
    for t in 0..n_trips {
        trips += &format!("R,S,trip_{t:07}\n");
        for i in 0..per_trip {
            let tm = 6 * 3600 + t % 1000 * 60 + i * 60;
            st += &format!(
                "trip_{t:07},{:02}:{:02}:{:02},{:02}:{:02}:{:02},stop_{:05},{}\n",
                tm / 3600,
                tm / 60 % 60,
                tm % 60,
                tm / 3600,
                tm / 60 % 60,
                tm % 60,
                (t + i) % 2_000,
                i + 1
            );
        }
    }
    let bytes = st.len();
    let z = make_zip(&[
        ("stops.txt", stops.as_bytes()),
        ("routes.txt", ROUTES),
        ("trips.txt", trips.as_bytes()),
        ("stop_times.txt", st.as_bytes()),
        ("calendar.txt", CAL),
    ]);
    drop(st);
    let dir = std::env::temp_dir().join(format!("peabiru-review-mem-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("big.zip");
    std::fs::write(&p, z).unwrap();
    let t = Instant::now();
    let layer = TransitLayer::load(&[p]).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    eprintln!(
        "gtfs_memory_per_stop_time: {} stop_times ({} MB of CSV), {} patterns, loaded in {:?}",
        n_trips * per_trip,
        bytes >> 20,
        layer.patterns.len(),
        t.elapsed()
    );
}
