//! Park-and-ride connectors (R5 `buildParkAndRideAreas` / `buildParkAndRideNodes`)
//! and the edge splits they make.
//!
//! The connectors themselves are dead ends for routing (two LINK edges in a
//! row are not allowed), but the splits change street edges: a split edge is
//! two edges with two rounded traversal times, and an area split also drops
//! the geometry. The connectors also enter the search queue, which matters
//! for R5's tie order. Splits here follow R5 literally, including two unit
//! slips in `Split.findOnEdge` that make area splits snap to an edge end.

use super::flags::*;
use super::geom::{first_last_angles, java_int, radians_to_brads, segment_fraction, FIXED_FACTOR};
use super::link::{find_split, Split};
use super::{Mode, StreetNetwork};

/// Splits closer than this to an edge end reuse the end vertex (R5 `SNAP_RADIUS_MM`).
pub const SNAP_RADIUS_MM: i32 = 5_000;
const METERS_PER_DEGREE_LAT: f64 = 111_111.111;
/// R5 stores `EdgeStore.DEFAULT_SPEED_KPH` (50) raw as the speed of new
/// edges, which reads back as 0.5 m/s.
const NEW_EDGE_SPEED_CMS: i16 = 50;

/// Whether a `park_ride` tag value marks a park-and-ride (anything but "no").
pub(crate) fn is_park_and_ride_value(v: &str) -> bool {
    !v.eq_ignore_ascii_case("no")
}

/// Recompute the bearings of pair `k` from its current geometry.
fn compute_angles(net: &mut StreetNetwork, k: usize) {
    let pts: Vec<(f64, f64)> = net
        .pair_points(k)
        .map(|(lat, lon)| (f64::from(lon) / FIXED_FACTOR, f64::from(lat) / FIXED_FACTOR))
        .collect();
    let (first, last) = first_last_angles(&pts);
    net.pair_in_brad[k] = radians_to_brads(first);
    net.pair_out_brad[k] = radians_to_brads(last);
}

/// Add a vertex at fixed-point coordinates.
fn add_vertex_fixed(net: &mut StreetNetwork, lat: i32, lon: i32) -> u32 {
    net.v_lat.push(lat);
    net.v_lon.push(lon);
    net.v_flags.push(0);
    (net.v_lat.len() - 1) as u32
}

/// Add an edge pair with straight geometry (R5 `addStreetPair`); flags and
/// speeds are zero / R5's raw default until set.
fn add_pair(net: &mut StreetNetwork, from: u32, to: u32, length_mm: i32, way: i64) -> usize {
    let k = net.pair_from.len();
    net.pair_from.push(from);
    net.pair_to.push(to);
    net.pair_length_mm.push(length_mm);
    net.pair_way.push(way);
    let at = net.geom.len() as u32;
    net.pair_geom.push((at, at));
    net.pair_in_brad.push(0);
    net.pair_out_brad.push(0);
    net.edge_flags.extend([0, 0]);
    net.edge_speed_cms
        .extend([NEW_EDGE_SPEED_CMS, NEW_EDGE_SPEED_CMS]);
    compute_angles(net, k);
    k
}

/// Replace the intermediate geometry of pair `k`.
fn set_geometry(net: &mut StreetNetwork, k: usize, pts: &[(i32, i32)]) {
    let start = net.geom.len() as u32;
    net.geom.extend_from_slice(pts);
    net.pair_geom[k] = (start, net.geom.len() as u32);
    compute_angles(net, k);
}

/// A 1 mm connector usable by every mode (R5 `allowAllModes` + LINK).
fn add_link(net: &mut StreetNetwork, from: u32, to: u32, next_generated_id: &mut i64) {
    let way = -*next_generated_id;
    *next_generated_id += 1;
    let k = add_pair(net, from, to, 1, way);
    let f = ALLOWS_PEDESTRIAN | ALLOWS_BIKE | ALLOWS_CAR | LINK;
    net.edge_flags[2 * k] = f;
    net.edge_flags[2 * k + 1] = f;
}

/// Second half of a split pair: copies flags and speeds of both directions.
fn add_second_half(
    net: &mut StreetNetwork,
    k: usize,
    new_vertex: u32,
    old_to: u32,
    length_mm: i32,
) -> usize {
    let way = net.pair_way[k];
    let n = add_pair(net, new_vertex, old_to, length_mm, way);
    net.edge_flags[2 * n] = net.edge_flags[2 * k];
    net.edge_flags[2 * n + 1] = net.edge_flags[2 * k + 1];
    net.edge_speed_cms[2 * n] = net.edge_speed_cms[2 * k];
    net.edge_speed_cms[2 * n + 1] = net.edge_speed_cms[2 * k + 1];
    net.index.insert_extra(n as u32);
    n
}

/// R5 `getOrCreateVertexNear`: link a point for `mode` and, unless the split
/// is within 5 m of an end, cut the edge there keeping its geometry.
pub(crate) fn get_or_create_vertex_near(
    net: &mut StreetNetwork,
    lat: f64,
    lon: f64,
    mode: Mode,
) -> Option<u32> {
    let mut scratch = Vec::new();
    let s = find_split(net, lat, lon, mode, &mut scratch)?;
    let k = s.edge as usize / 2;
    if s.distance0_mm < SNAP_RADIUS_MM || s.distance1_mm < SNAP_RADIUS_MM {
        return Some(if s.distance0_mm < s.distance1_mm {
            net.pair_from[k]
        } else {
            net.pair_to[k]
        });
    }
    let v = add_vertex_fixed(net, s.fixed_lat, s.fixed_lon);
    let old_to = net.pair_to[k];
    let (a, b) = net.pair_geom[k];
    let mid: Vec<(i32, i32)> = net.geom[a as usize..b as usize].to_vec();
    let seg = s.seg as usize;
    let (pre, post) = if mid.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        (
            mid[..seg.min(mid.len())].to_vec(),
            mid[seg.min(mid.len())..].to_vec(),
        )
    };
    net.pair_length_mm[k] = s.distance0_mm;
    net.pair_to[k] = v;
    set_geometry(net, k, &pre);
    let n = add_second_half(net, k, v, old_to, s.distance1_mm);
    set_geometry(net, n, &post);
    Some(v)
}

/// R5 `Split.findOnEdge`, slips included: the "best" segment is the
/// shortest one (it compares segment lengths, not distances to the point)
/// and the distance along the edge is computed in fixed-point degrees times
/// metres per degree, so it is almost always clamped to the edge length.
fn find_on_edge(net: &StreetNetwork, lat: f64, lon: f64, k: usize) -> Split {
    let fixed_lat = java_int(lat * FIXED_FACTOR);
    let fixed_lon = java_int(lon * FIXED_FACTOR);
    let cos_lat = lat.to_radians().cos();
    let pts: Vec<(i32, i32)> = net.pair_points(k).collect();
    let mut best = (i64::MAX, 0usize, 0i32, 0i32, 0i32); // (d2, seg, lat, lon, distance0)
    let mut before = 0.0f64;
    for (seg, w) in pts.windows(2).enumerate() {
        let ((lat0, lon0), (lat1, lon1)) = (w[0], w[1]);
        let frac = segment_fraction(lon0, lat0, lon1, lat1, fixed_lon, fixed_lat, cos_lat);
        let p_lon = java_int(f64::from(lon0) + frac * f64::from(lon1 - lon0));
        let p_lat = java_int(f64::from(lat0) + frac * f64::from(lat1 - lat0));
        let dx = f64::from(lon1.wrapping_sub(lon0)) * cos_lat;
        let dy = f64::from(lat1.wrapping_sub(lat0));
        let length = (dx * dx + dy * dy).sqrt();
        let d0 = java_int((before + length * frac) * METERS_PER_DEGREE_LAT * 1000.0);
        before += length;
        let d2 = (dx * dx + dy * dy) as i64;
        if d2 < best.0 {
            best = (d2, seg, p_lat, p_lon, d0);
        }
    }
    let len = net.pair_length_mm[k];
    let (d0, d1) = if best.4 > len {
        (len, 0)
    } else {
        (best.4, len - best.4)
    };
    Split {
        edge: (2 * k) as u32,
        vertex0: net.pair_from[k],
        vertex1: net.pair_to[k],
        distance0_mm: d0,
        distance1_mm: d1,
        distance_to_edge_mm: 0,
        seg: best.1 as u32,
        fixed_lat: best.2,
        fixed_lon: best.3,
    }
}

/// R5 `splitEdge`: like [`get_or_create_vertex_near`] but both halves lose
/// their geometry.
fn split_edge(net: &mut StreetNetwork, s: &Split) -> u32 {
    let k = s.edge as usize / 2;
    if s.distance0_mm < SNAP_RADIUS_MM || s.distance1_mm < SNAP_RADIUS_MM {
        return if s.distance0_mm < s.distance1_mm {
            net.pair_from[k]
        } else {
            net.pair_to[k]
        };
    }
    let v = add_vertex_fixed(net, s.fixed_lat, s.fixed_lon);
    let old_to = net.pair_to[k];
    net.pair_length_mm[k] = s.distance0_mm;
    net.pair_to[k] = v;
    set_geometry(net, k, &[]);
    add_second_half(net, k, v, old_to, s.distance1_mm);
    v
}

/// One component of a line-line intersection.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Component {
    Point(f64, f64),
    Line,
}

/// Intersection of two polylines (points as `(x, y)`), as JTS 1.13
/// `LineString.intersection` lists it: isolated points sorted by x then y,
/// then one line per collinear overlap.
fn line_intersection(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<Component> {
    let mut points: Vec<(f64, f64)> = Vec::new();
    let mut overlaps: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for sa in a.windows(2) {
        for sb in b.windows(2) {
            match segment_intersection(sa[0], sa[1], sb[0], sb[1]) {
                SegX::None => {}
                SegX::Point(p) => points.push(p),
                SegX::Overlap(p, q) => overlaps.push((p, q)),
            }
        }
    }
    let on_overlap = |p: (f64, f64)| overlaps.iter().any(|&(u, v)| on_segment(p, u, v));
    points.retain(|&p| !on_overlap(p));
    points.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.total_cmp(&q.1)));
    points.dedup();
    let mut out: Vec<Component> = points
        .into_iter()
        .map(|(x, y)| Component::Point(x, y))
        .collect();
    // Contiguous overlaps merge into one line in JTS; count connected runs.
    let mut runs = 0;
    let mut seen = vec![false; overlaps.len()];
    for i in 0..overlaps.len() {
        if seen[i] {
            continue;
        }
        runs += 1;
        let mut stack = vec![i];
        seen[i] = true;
        while let Some(j) = stack.pop() {
            for m in 0..overlaps.len() {
                if !seen[m] && touches(overlaps[j], overlaps[m]) {
                    seen[m] = true;
                    stack.push(m);
                }
            }
        }
    }
    out.extend(std::iter::repeat(Component::Line).take(runs));
    out
}

fn touches(a: ((f64, f64), (f64, f64)), b: ((f64, f64), (f64, f64))) -> bool {
    a.0 == b.0 || a.0 == b.1 || a.1 == b.0 || a.1 == b.1
}

enum SegX {
    None,
    Point((f64, f64)),
    Overlap((f64, f64), (f64, f64)),
}

fn cross(o: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

fn on_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> bool {
    cross(a, b, p) == 0.0
        && p.0 >= a.0.min(b.0)
        && p.0 <= a.0.max(b.0)
        && p.1 >= a.1.min(b.1)
        && p.1 <= a.1.max(b.1)
}

fn segment_intersection(p1: (f64, f64), p2: (f64, f64), q1: (f64, f64), q2: (f64, f64)) -> SegX {
    let d1 = cross(q1, q2, p1);
    let d2 = cross(q1, q2, p2);
    let d3 = cross(p1, p2, q1);
    let d4 = cross(p1, p2, q2);
    if d1 == 0.0 && d2 == 0.0 {
        // collinear: overlap of the projections
        let along = |p: (f64, f64)| {
            if (p2.0 - p1.0).abs() >= (p2.1 - p1.1).abs() {
                p.0
            } else {
                p.1
            }
        };
        let mut ends = [
            (along(p1), p1),
            (along(p2), p2),
            (along(q1), q1),
            (along(q2), q2),
        ];
        let (pa, pb) = (along(p1).min(along(p2)), along(p1).max(along(p2)));
        let (qa, qb) = (along(q1).min(along(q2)), along(q1).max(along(q2)));
        let lo = pa.max(qa);
        let hi = pb.min(qb);
        if lo > hi {
            return SegX::None;
        }
        ends.sort_by(|a, b| a.0.total_cmp(&b.0));
        let at = |t: f64| ends.iter().find(|e| e.0 == t).map(|e| e.1).unwrap();
        return if lo == hi {
            SegX::Point(at(lo))
        } else {
            SegX::Overlap(at(lo), at(hi))
        };
    }
    let straddles = |a: f64, b: f64| (a > 0.0 && b < 0.0) || (a < 0.0 && b > 0.0);
    if straddles(d1, d2) && straddles(d3, d4) {
        let t = d1 / (d1 - d2);
        return SegX::Point((p1.0 + t * (p2.0 - p1.0), p1.1 + t * (p2.1 - p1.1)));
    }
    for (p, a, b) in [(p1, q1, q2), (p2, q1, q2), (q1, p1, p2), (q2, p1, p2)] {
        if on_segment(p, a, b) {
            return SegX::Point(p);
        }
    }
    SegX::None
}

/// Centroid of a polyline as JTS computes it: segment midpoints weighted by
/// length, or the first point when the line has no length.
fn line_centroid(pts: &[(f64, f64)]) -> (f64, f64) {
    let (mut sx, mut sy, mut total) = (0.0, 0.0, 0.0);
    for w in pts.windows(2) {
        let len = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
        total += len;
        sx += len * (w[0].0 + w[1].0) / 2.0;
        sy += len * (w[0].1 + w[1].1) / 2.0;
    }
    if total > 0.0 {
        (sx / total, sy / total)
    } else {
        pts[0]
    }
}

/// Add park-and-ride connectors. `areas` are the node coordinates of each
/// park-and-ride way as `(lat, lon)` R5 fixed-point, in way id order;
/// `nodes` are park-and-ride nodes as `(lat, lon)` degrees, in node id order.
pub(crate) fn apply_park_and_ride(
    net: &mut StreetNetwork,
    areas: &[Vec<(i32, i32)>],
    nodes: &[(f64, f64)],
) {
    let mut next_id = 1i64;
    let mut scratch = Vec::new();
    for area in areas {
        if area.is_empty() {
            continue;
        }
        let coords: Vec<(f64, f64)> = area
            .iter()
            .map(|&(lat, lon)| (f64::from(lon) / FIXED_FACTOR, f64::from(lat) / FIXED_FACTOR))
            .collect();
        let (cx, cy) = line_centroid(&coords);
        let center = add_vertex_fixed(
            net,
            java_int(cy * FIXED_FACTOR),
            java_int(cx * FIXED_FACTOR),
        );
        let fixed = |d: f64| i64::from(java_int(d * FIXED_FACTOR));
        let min_x = coords.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
        let max_x = coords.iter().map(|c| c.0).fold(f64::NEG_INFINITY, f64::max);
        let min_y = coords.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
        let max_y = coords.iter().map(|c| c.1).fold(f64::NEG_INFINITY, f64::max);
        net.index.query(
            fixed(min_y),
            fixed(min_x),
            fixed(max_y),
            fixed(max_x),
            &mut scratch,
        );
        scratch.sort_unstable();
        scratch.dedup();
        let candidates = scratch.clone();
        for k in candidates {
            let k = k as usize;
            let f = net.edge_flags[2 * k];
            if f & LINKABLE == 0 || f & LINK != 0 {
                continue;
            }
            let edge: Vec<(f64, f64)> = net
                .pair_points(k)
                .map(|(lat, lon)| (f64::from(lon) / FIXED_FACTOR, f64::from(lat) / FIXED_FACTOR))
                .collect();
            for c in line_intersection(&edge, &coords) {
                let targets: Vec<(f64, f64)> = match c {
                    Component::Point(x, y) => vec![(x, y)],
                    // R5 uses the park-and-ride way's own ends here
                    Component::Line => vec![coords[0], coords[coords.len() - 1]],
                };
                for (x, y) in targets {
                    let s = find_on_edge(net, y, x, k);
                    let target = split_edge(net, &s);
                    add_link(net, center, target, &mut next_id);
                }
            }
        }
    }
    for &(lat, lon) in nodes {
        let center = add_vertex_fixed(
            net,
            java_int(lat * FIXED_FACTOR),
            java_int(lon * FIXED_FACTOR),
        );
        let Some(walking) = get_or_create_vertex_near(net, lat, lon, Mode::Walk) else {
            continue;
        };
        add_link(net, center, walking, &mut next_id);
        let Some(driving) = get_or_create_vertex_near(net, lat, lon, Mode::Car) else {
            continue; // R5 links the car side only if one is found
        };
        if driving == walking {
            continue;
        }
        add_link(net, center, driving, &mut next_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossing_lines_meet_in_one_point() {
        let a = [(0.0, 0.0), (2.0, 2.0)];
        let b = [(0.0, 2.0), (2.0, 0.0)];
        assert_eq!(line_intersection(&a, &b), vec![Component::Point(1.0, 1.0)]);
    }

    #[test]
    fn touching_and_overlapping_lines() {
        // b touches a at a vertex
        let a = [(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)];
        let b = [(1.0, 0.0), (1.0, 1.0)];
        assert_eq!(line_intersection(&a, &b), vec![Component::Point(1.0, 0.0)]);
        // collinear overlap over two segments is one line
        let c = [(0.5, 0.0), (1.5, 0.0)];
        assert_eq!(line_intersection(&a, &c), vec![Component::Line]);
        // disjoint
        let d = [(0.0, 1.0), (2.0, 1.0)];
        assert!(line_intersection(&a, &d).is_empty());
    }

    #[test]
    fn centroid_weights_segments_by_length() {
        let c = line_centroid(&[(0.0, 0.0), (2.0, 0.0), (2.0, 1.0)]);
        // segments of length 2 (midpoint (1,0)) and 1 (midpoint (2,0.5))
        assert!((c.0 - 4.0 / 3.0).abs() < 1e-12 && (c.1 - 0.5 / 3.0).abs() < 1e-12);
    }
}

#[cfg(test)]
mod network_tests {
    use crate::osm::pbf::{Extract, Tags, Way};
    use crate::street::build::network_from_extract;
    use crate::street::flags::LINK;
    use crate::street::router::{street_seconds, Mode};

    /// A 7 x 7 grid of residential streets 1e-3 degrees apart, optionally
    /// with a park-and-ride node beside the middle of the bottom-left street.
    fn grid(park_ride: bool) -> Extract {
        let n = 7;
        let id = |r: usize, c: usize| (1 + r * n + c) as i64;
        let tags = || Tags(vec![("highway".into(), "residential".into())]);
        let mut ways = Vec::new();
        for r in 0..n {
            ways.push(Way {
                id: 1000 + r as i64,
                tags: tags(),
                nodes: (0..n).map(|c| id(r, c)).collect(),
            });
        }
        for c in 0..n {
            ways.push(Way {
                id: 2000 + c as i64,
                tags: tags(),
                nodes: (0..n).map(|r| id(r, c)).collect(),
            });
        }
        let node_ids: Vec<i64> = (0..n * n).map(|i| 1 + i as i64).collect();
        let pois = if park_ride {
            // 20 m north of the street between (1,1) and (1,2): lat 1e-3, lon 1.5e-3
            vec![(
                9_999,
                10_000 + 180,
                15_000,
                Tags(vec![("park_ride".into(), "yes".into())]),
            )]
        } else {
            Vec::new()
        };
        Extract {
            ref_counts: vec![2; node_ids.len()],
            lat: node_ids
                .iter()
                .map(|&i| (((i - 1) as usize / n) * 10_000) as i32)
                .collect(),
            lon: node_ids
                .iter()
                .map(|&i| (((i - 1) as usize % n) * 10_000) as i32)
                .collect(),
            node_ids,
            ways,
            pois,
            ..Extract::default()
        }
    }

    #[test]
    fn park_and_ride_node_splits_the_nearest_street_and_links_to_it() {
        let plain = network_from_extract(grid(false));
        let net = network_from_extract(grid(true));
        // one split (a new pair) and one connector pair; walk and car link
        // to the same new vertex, so there is no second connector
        assert_eq!(net.pair_from.len(), plain.pair_from.len() + 2);
        assert_eq!(net.n_vertices(), plain.n_vertices() + 2);
        let links = (0..net.n_edges())
            .filter(|&e| net.edge_flags[e] & LINK != 0)
            .count();
        assert_eq!(links, 2);
        // the split pair keeps its length in total
        let total = |n: &crate::street::StreetNetwork| {
            n.pair_length_mm.iter().map(|&l| i64::from(l)).sum::<i64>()
        };
        assert!((total(&net) - 1 - total(&plain)).abs() <= 1);
        // the connector is a dead end: a car trip past it is unchanged or
        // differs only by the extra rounding of the split street
        let from = (1e-3 + 2e-5, 0.3e-3);
        let to = (1e-3 + 2e-5, 5.7e-3);
        let a = street_seconds(&plain, from, to, Mode::Car).unwrap();
        let b = street_seconds(&net, from, to, Mode::Car).unwrap();
        assert!((0..=1).contains(&(b - a)), "{a} vs {b}");
    }
}
