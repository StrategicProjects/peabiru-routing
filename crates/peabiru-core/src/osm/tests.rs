//! PBF reader tests on files built by hand with the test encoder.

use super::pbf::*;
use super::protobuf::encode::*;

/// Strings used by the test block; index 0 is the conventional empty string.
const STRINGS: [&str; 9] = [
    "",
    "highway",
    "residential",
    "barrier",
    "gate",
    "building",
    "yes",
    "type",
    "restriction",
];

fn s(x: &str) -> u64 {
    STRINGS.iter().position(|&y| y == x).unwrap() as u64
}

fn primitive_block() -> Vec<u8> {
    let mut st = Vec::new();
    for x in STRINGS {
        field_bytes(&mut st, 1, x.as_bytes());
    }
    // Dense nodes 1, 2, 3 (node 2 tagged barrier=gate); granularity 100.
    let mut dense = Vec::new();
    field_bytes(&mut dense, 1, &packed_delta(&[1, 2, 3]));
    field_bytes(
        &mut dense,
        8,
        &packed_delta(&[-80_000_000, -80_000_100, -80_000_200]),
    );
    field_bytes(
        &mut dense,
        9,
        &packed_delta(&[-349_000_000, -349_000_100, -349_000_201]),
    );
    field_bytes(&mut dense, 10, &packed([0, s("barrier"), s("gate"), 0, 0]));
    // A plain node 9, referenced by nothing.
    let mut node = Vec::new();
    field_varint(&mut node, 1, zz(9));
    field_varint(&mut node, 8, zz(10));
    field_varint(&mut node, 9, zz(20));
    // Way 10: highway, nodes 1-2-3-4 (4 is outside the extract).
    let mut way = Vec::new();
    field_varint(&mut way, 1, 10);
    field_bytes(&mut way, 2, &packed([s("highway")]));
    field_bytes(&mut way, 3, &packed([s("residential")]));
    field_bytes(&mut way, 8, &packed_delta(&[1, 2, 3, 4]));
    // Way 11: a building, filtered out.
    let mut building = Vec::new();
    field_varint(&mut building, 1, 11);
    field_bytes(&mut building, 2, &packed([s("building")]));
    field_bytes(&mut building, 3, &packed([s("yes")]));
    field_bytes(&mut building, 8, &packed_delta(&[9, 3]));
    // Relation 20: a restriction with way 10 as "from" and node 2 as "via".
    let mut rel = Vec::new();
    field_varint(&mut rel, 1, 20);
    field_bytes(&mut rel, 2, &packed([s("type")]));
    field_bytes(&mut rel, 3, &packed([s("restriction")]));
    field_bytes(&mut rel, 8, &packed([s(""), s("")]));
    field_bytes(&mut rel, 9, &packed_delta(&[10, 2]));
    field_bytes(&mut rel, 10, &packed([1, 0]));

    let mut g1 = Vec::new();
    field_bytes(&mut g1, 2, &dense);
    let mut g2 = Vec::new();
    field_bytes(&mut g2, 1, &node);
    let mut g3 = Vec::new();
    field_bytes(&mut g3, 3, &way);
    field_bytes(&mut g3, 3, &building);
    let mut g4 = Vec::new();
    field_bytes(&mut g4, 4, &rel);

    let mut block = Vec::new();
    field_bytes(&mut block, 1, &st);
    for g in [g1, g2, g3, g4] {
        field_bytes(&mut block, 2, &g);
    }
    block
}

fn blob(kind: &str, payload: &[u8], zlib: bool) -> Vec<u8> {
    let mut b = Vec::new();
    if zlib {
        field_varint(&mut b, 2, payload.len() as u64);
        field_bytes(
            &mut b,
            3,
            &miniz_oxide::deflate::compress_to_vec_zlib(payload, 6),
        );
    } else {
        field_bytes(&mut b, 1, payload);
    }
    let mut header = Vec::new();
    field_bytes(&mut header, 1, kind.as_bytes());
    field_varint(&mut header, 3, b.len() as u64);
    let mut out = (header.len() as u32).to_be_bytes().to_vec();
    out.extend(header);
    out.extend(b);
    out
}

fn file(zlib: bool, features: &[&str]) -> Vec<u8> {
    let mut hb = Vec::new();
    for f in features {
        field_bytes(&mut hb, 4, f.as_bytes());
    }
    let mut out = blob("OSMHeader", &hb, zlib);
    out.extend(blob("OSMData", &primitive_block(), zlib));
    out
}

fn highways(t: &[(&str, &str)]) -> bool {
    tag(t, "highway").is_some()
}

fn restrictions(t: &[(&str, &str)]) -> bool {
    tag(t, "type") == Some("restriction")
}

fn barriers(t: &[(&str, &str)]) -> bool {
    tag(t, "barrier").is_some()
}

fn opts(n_threads: usize) -> ReadOptions<'static> {
    ReadOptions {
        way_filter: &highways,
        relation_filter: Some(&restrictions),
        node_tag_filter: Some(&barriers),
        n_threads,
    }
}

#[test]
fn reads_ways_relations_and_only_referenced_nodes() {
    for zlib in [false, true] {
        for threads in [1, 4] {
            let ex = read_pbf(
                &file(zlib, &["OsmSchema-V0.6", "DenseNodes"]),
                &opts(threads),
            )
            .unwrap();
            assert_eq!(ex.ways.len(), 1);
            let w = &ex.ways[0];
            assert_eq!((w.id, w.nodes.clone()), (10, vec![1, 2, 3, 4]));
            assert_eq!(w.tags.get("highway"), Some("residential"));

            // node 9 belongs only to the filtered-out building
            assert_eq!(ex.node_ids, vec![1, 2, 3, 4]);
            // 1e-7 degrees: raw nanodegrees * granularity 100 / 100
            assert_eq!(ex.lat[..3], [-80_000_000, -80_000_100, -80_000_200]);
            assert_eq!(ex.lon[..3], [-349_000_000, -349_000_100, -349_000_201]);
            // node 4 is not in the file
            assert_eq!((ex.lat[3], ex.lon[3]), (i32::MIN, i32::MIN));

            assert_eq!(ex.node_tags.len(), 1);
            assert_eq!(ex.node_tags[0].0, 2);
            assert_eq!(ex.node_tags[0].1.get("barrier"), Some("gate"));

            assert_eq!(ex.relations.len(), 1);
            let r = &ex.relations[0];
            assert_eq!(
                r.members[0],
                Member {
                    kind: MemberKind::Way,
                    id: 10,
                    role: String::new()
                }
            );
            assert_eq!(r.members[1].kind, MemberKind::Node);
            assert_eq!(r.members[1].id, 2);
        }
    }
}

#[test]
fn rejects_unsupported_required_features() {
    let err = read_pbf(&file(false, &["HistoricalInformation"]), &opts(1)).unwrap_err();
    assert!(err.to_string().contains("HistoricalInformation"));
}

#[test]
fn truncated_file_is_an_error() {
    let mut f = file(true, &["OsmSchema-V0.6"]);
    f.truncate(f.len() - 3);
    assert!(read_pbf(&f, &opts(1)).is_err());
}
