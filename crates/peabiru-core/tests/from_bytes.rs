//! A network built from bytes in memory is the one built from the same
//! files in a directory.

use peabiru_core::{
    build_from_bytes, build_from_dir, street_travel_time_matrix, transit_travel_time_matrix,
    BuildOptions, Mode, RouteParams, TransitData, TransitParams,
};

const SPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../r/inst/extdata/spo");

fn points() -> Vec<(f64, f64)> {
    let text = std::fs::read_to_string(format!("{SPO}/spo_points.csv")).unwrap();
    text.lines()
        .skip(1)
        .map(|line| {
            let f: Vec<&str> = line.split(',').collect();
            (f[2].parse().unwrap(), f[1].parse().unwrap())
        })
        .collect()
}

fn read(name: &str) -> Vec<u8> {
    std::fs::read(format!("{SPO}/{name}")).unwrap()
}

#[test]
fn bytes_build_the_network_of_the_directory() {
    let opts = BuildOptions::default();
    let (net_d, layer_d) = build_from_dir(SPO, &opts).unwrap();
    let (pbf, gtfs) = (read("spo_osm.pbf"), read("spo_gtfs.zip"));
    let (net_b, layer_b) = build_from_bytes(&pbf, &[("spo_gtfs.zip", &gtfs)], &opts).unwrap();

    assert_eq!(net_b.n_vertices(), net_d.n_vertices());
    assert_eq!(net_b.n_edges(), net_d.n_edges());
    assert_eq!(layer_b.feed_ids(), layer_d.feed_ids());
    assert_eq!(layer_b.n_stops(), layer_d.n_stops());
    assert_eq!(layer_b.n_linked_stops(), layer_d.n_linked_stops());
    assert_eq!(layer_b.n_patterns(), layer_d.n_patterns());
    assert_eq!(layer_b.dropped_trips(), layer_d.dropped_trips());
    let (rb, rd) = (&layer_b.feed_reports()[0], &layer_d.feed_reports()[0]);
    assert_eq!(rb.file_name(), rd.file_name());
    assert_eq!(rb.r5r_refusals(), rd.r5r_refusals());

    let pts = points();
    for mode in [Mode::Walk, Mode::Bicycle, Mode::Car] {
        let p = RouteParams::default();
        let b = street_travel_time_matrix(&net_b, &pts, &pts, mode, &p).unwrap();
        let d = street_travel_time_matrix(&net_d, &pts, &pts, mode, &p).unwrap();
        assert_eq!(b.as_slice(), d.as_slice(), "{mode:?}");
    }
    let mut p = TransitParams::default();
    p.date = 20190513;
    p.departure_time = 14 * 3600;
    p.percentiles = vec![10, 50, 90];
    p.seed = Some(42);
    let data_b = TransitData::new(&net_b, &layer_b);
    let data_d = TransitData::new(&net_d, &layer_d);
    let b = transit_travel_time_matrix(&net_b, &layer_b, &data_b, &pts, &pts, &p).unwrap();
    let d = transit_travel_time_matrix(&net_d, &layer_d, &data_d, &pts, &pts, &p).unwrap();
    assert_eq!(b, d);
}

#[test]
fn feeds_load_in_the_order_given() {
    let (pbf, gtfs) = (read("spo_osm.pbf"), read("spo_gtfs.zip"));
    let opts = BuildOptions::default();
    let feeds: [(&str, &[u8]); 2] = [("b.zip", &gtfs), ("a.zip", &gtfs)];
    let (_, layer) = build_from_bytes(&pbf, &feeds, &opts).unwrap();
    // spo_gtfs.zip has no feed_info.txt: the ids are the names
    assert_eq!(layer.feed_ids(), ["b", "a"]);
    let names: Vec<String> = layer.feed_reports().iter().map(|r| r.file_name()).collect();
    assert_eq!(names, ["b.zip", "a.zip"]);
    let (_, none) = build_from_bytes(&pbf, &[], &opts).unwrap();
    assert!(none.feed_ids().is_empty());
}

#[test]
fn errors_name_the_data() {
    let opts = BuildOptions::default();
    let e = build_from_bytes(b"not a pbf", &[], &opts).unwrap_err();
    assert!(e.to_string().starts_with("OpenStreetMap data: "), "{e}");
    let pbf = read("spo_osm.pbf");
    let e = build_from_bytes(&pbf, &[("feed.zip", b"not a zip")], &opts).unwrap_err();
    assert!(e.to_string().starts_with("feed.zip: "), "{e}");
}
