//! Walking distances around stops, precomputed once per network: transfers
//! between stops (R5 `TransferFinder`) and the stop-to-vertex distance trees
//! that egress to destinations is built from
//! (`TransitLayer.buildDistanceTables`, `EgressCostTable`).

use super::TransitLayer;
use crate::jcompat::TroveIntKeys;
use crate::street::flags::{ALLOWS_PEDESTRIAN, LINK};
use crate::street::geom::FIXED_FACTOR;
use crate::street::link::Split;
use crate::street::StreetNetwork;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Transfer search limit (R5 `TRANSFER_DISTANCE_LIMIT_METERS`), inclusive.
pub const TRANSFER_LIMIT_MM: i32 = 1_000_000;
/// Egress tree limit (R5 `WALK_DISTANCE_LIMIT_METERS`), inclusive.
pub const EGRESS_LIMIT_MM: i32 = 2_000_000;
const EGRESS_LIMIT_M: f64 = 2000.0;
/// R5 `SphericalDistanceLibrary.EARTH_CIRCUMFERENCE_METERS`.
const EARTH_CIRCUMFERENCE_M: f64 = 40_041.438_5 * 1000.0;

/// Shortest walking distance (mm) from a vertex to every vertex within
/// `limit_mm`, as R5's distance-minimising `StreetRouter` from a vertex
/// origin: edge-based, a vertex's distance is the best state on its incoming
/// edges, zero-length edges count 1 mm, and two LINK edges in a row are not
/// allowed after the first edge. Returns `(vertex, mm)` sorted by vertex.
pub fn walk_distance_tree(net: &StreetNetwork, origin: usize, limit_mm: i32) -> Vec<(u32, i32)> {
    let mut best_edge: std::collections::HashMap<u32, i32> = std::collections::HashMap::new();
    let mut heap: BinaryHeap<Reverse<(i32, u32)>> = BinaryHeap::new();
    let relax = |from_edge: Option<usize>,
                 d: i32,
                 v: usize,
                 best_edge: &mut std::collections::HashMap<u32, i32>,
                 heap: &mut BinaryHeap<Reverse<(i32, u32)>>| {
        for &f in net.out_edges(v) {
            let flags = net.edge_flags[f as usize];
            if flags & ALLOWS_PEDESTRIAN == 0 {
                continue;
            }
            if let Some(e) = from_edge {
                if flags & LINK != 0 && net.edge_flags[e] & LINK != 0 {
                    continue;
                }
            }
            let mut d1 = d.saturating_add(net.pair_length_mm[f as usize / 2]);
            if d1 == d {
                d1 += 1;
            }
            if d1 > limit_mm {
                continue;
            }
            let cur = best_edge.get(&f).copied().unwrap_or(i32::MAX);
            if d1 < cur {
                best_edge.insert(f, d1);
                heap.push(Reverse((d1, f)));
            }
        }
    };
    relax(None, 0, origin, &mut best_edge, &mut heap);
    while let Some(Reverse((d, e))) = heap.pop() {
        if d > best_edge[&e] {
            continue;
        }
        relax(
            Some(e as usize),
            d,
            net.edge_to(e as usize),
            &mut best_edge,
            &mut heap,
        );
    }
    let mut by_vertex: std::collections::HashMap<u32, i32> = std::collections::HashMap::new();
    for (e, d) in best_edge {
        let v = net.edge_to(e as usize) as u32;
        let slot = by_vertex.entry(v).or_insert(i32::MAX);
        *slot = (*slot).min(d);
    }
    let mut out: Vec<(u32, i32)> = by_vertex.into_iter().collect();
    out.sort_unstable();
    out
}

/// Distance trees (2 km) for every linked stop; `None` for unlinked stops.
pub fn stop_trees(net: &StreetNetwork, layer: &TransitLayer) -> Vec<Option<Vec<(u32, i32)>>> {
    layer
        .stop_vertex
        .iter()
        .map(|&v| (v >= 0).then(|| walk_distance_tree(net, v as usize, EGRESS_LIMIT_MM)))
        .collect()
}

#[inline]
fn tree_get(tree: &[(u32, i32)], v: u32) -> i32 {
    tree.binary_search_by_key(&v, |&(x, _)| x)
        .map_or(i32::MAX, |i| tree[i].1)
}

/// Transfers from each stop: `(target stop, distance mm)`, keeping only the
/// closest stop of each pattern reached within 1 km (R5 `TransferFinder`),
/// with R5's tie order (Trove hash-map iteration).
pub fn find_transfers(
    layer: &TransitLayer,
    trees: &[Option<Vec<(u32, i32)>>],
) -> Vec<Vec<(u32, i32)>> {
    let n = layer.stop_ids.len();
    // stopForStreetVertex: TIntIntHashMap(nStops, 0.5), keys = stop vertices
    // inserted in stop order (unlinked stops put key -1, skipped later).
    let mut by_vertex = TroveIntKeys::with_capacity(n, 0.5);
    let mut stop_of_vertex = std::collections::HashMap::new();
    for (s, &v) in layer.stop_vertex.iter().enumerate() {
        by_vertex.insert(v);
        stop_of_vertex.insert(v, s as u32); // a later put overwrites
    }
    let vertex_order: Vec<i32> = by_vertex.iteration_order().filter(|&v| v != -1).collect();

    let mut transfers = vec![Vec::new(); n];
    for (s, tree) in trees.iter().enumerate() {
        let Some(tree) = tree else { continue };
        // getReachedStops: a TIntIntHashMap filled in stopForStreetVertex order
        let mut reached = TroveIntKeys::new();
        let mut dist = std::collections::HashMap::new();
        for &v in &vertex_order {
            let d = tree_get(tree, v as u32);
            if d <= TRANSFER_LIMIT_MM {
                let stop = stop_of_vertex[&v];
                reached.insert(stop as i32);
                dist.insert(stop, d);
            }
        }
        dist.remove(&(s as u32)); // the source itself
                                  // retainClosestStopsOnPatterns: first stop seen wins a tie
        let mut best_on_pattern: std::collections::HashMap<u32, u32> =
            std::collections::HashMap::new();
        let order: Vec<u32> = reached
            .iteration_order()
            .map(|k| k as u32)
            .filter(|k| dist.contains_key(k))
            .collect();
        for &stop in &order {
            let d = dist[&stop];
            for &p in &layer.patterns_for_stop[stop as usize] {
                match best_on_pattern.get(&p) {
                    None => {
                        best_on_pattern.insert(p, stop);
                    }
                    Some(&cur) if dist[&cur] > d => {
                        best_on_pattern.insert(p, stop);
                    }
                    _ => {}
                }
            }
        }
        let keep: std::collections::HashSet<u32> = best_on_pattern.into_values().collect();
        transfers[s] = order
            .into_iter()
            .filter(|t| keep.contains(t))
            .map(|t| (t, dist[&t]))
            .collect();
    }
    transfers
}

/// Egress distances (mm) from stops to each destination point, as R5's
/// `EgressCostTable` for WALK: per stop, points strictly inside the stop's
/// envelope grown by 2 km, cost through the nearer end of the point's
/// linked edge. Returns, per destination, `(stop, mm)` pairs.
pub fn egress_tables(
    layer: &TransitLayer,
    trees: &[Option<Vec<(u32, i32)>>],
    net: &StreetNetwork,
    dest_points: &[(f64, f64)],
    dest_splits: &[Option<Split>],
) -> Vec<Vec<(u32, i32)>> {
    let mut out = vec![Vec::new(); dest_points.len()];
    for (s, tree) in trees.iter().enumerate() {
        let Some(tree) = tree else { continue };
        let v = layer.stop_vertex[s] as usize;
        let (flat, flon) = (f64::from(net.v_lat[v]), f64::from(net.v_lon[v]));
        let lat_exp = ((EGRESS_LIMIT_M / EARTH_CIRCUMFERENCE_M * 360.0) * FIXED_FACTOR) as i32;
        let cos_lat = (flat.abs() / FIXED_FACTOR).to_radians().cos();
        let lon_exp =
            ((EGRESS_LIMIT_M / EARTH_CIRCUMFERENCE_M * 360.0 / cos_lat) * FIXED_FACTOR) as i32;
        let north = (flat + f64::from(lat_exp)) / FIXED_FACTOR;
        let south = (flat - f64::from(lat_exp)) / FIXED_FACTOR;
        let east = (flon + f64::from(lon_exp)) / FIXED_FACTOR;
        let west = (flon - f64::from(lon_exp)) / FIXED_FACTOR;
        for (p, &(lat, lon)) in dest_points.iter().enumerate() {
            if !(lat < north && lat > south && lon < east && lon > west) {
                continue;
            }
            let Some(sp) = dest_splits[p] else { continue };
            let d0 = tree_get(tree, sp.vertex0);
            let d1 = tree_get(tree, sp.vertex1);
            if d0 == i32::MAX && d1 == i32::MAX {
                continue;
            }
            let via = |d: i32, along: i32| {
                if d == i32::MAX {
                    i32::MAX
                } else {
                    let x = d.wrapping_add(along).wrapping_add(sp.distance_to_edge_mm);
                    if x < 0 {
                        i32::MAX
                    } else {
                        x
                    }
                }
            };
            let c = via(d0, sp.distance0_mm).min(via(d1, sp.distance1_mm));
            if c != i32::MAX {
                out[p].push((s as u32, c));
            }
        }
    }
    out
}
