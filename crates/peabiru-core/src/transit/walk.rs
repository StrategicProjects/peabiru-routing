//! Walking distances around stops, precomputed once per network: transfers
//! between stops (R5 `TransferFinder`) and the stop-to-vertex distance trees
//! that egress to destinations is built from
//! (`TransitLayer.buildDistanceTables`, `EgressCostTable`).

use super::TransitLayer;
use crate::jcompat::TroveIntKeys;
use crate::street::flags::{ALLOWS_PEDESTRIAN, LINK};
use crate::street::geom::FIXED_FACTOR;
use crate::street::link::Split;
use crate::street::router::RadixHeap;
use crate::street::StreetNetwork;

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
    walk_distance_tree_with(net, origin, limit_mm, &mut TreeScratch::new(net))
}

/// Buffers for [`walk_distance_tree`], reused from tree to tree.
pub(crate) struct TreeScratch {
    best_edge: Vec<i32>,
    touched_edges: Vec<u32>,
    best_vertex: Vec<i32>,
    heap: RadixHeap,
}

impl TreeScratch {
    pub(crate) fn new(net: &StreetNetwork) -> Self {
        TreeScratch {
            best_edge: vec![i32::MAX; net.n_edges()],
            touched_edges: Vec::new(),
            best_vertex: vec![i32::MAX; net.n_vertices()],
            heap: RadixHeap::default(),
        }
    }
}

/// [`walk_distance_tree`] with reused buffers. The distances are shortest
/// path lengths, so they do not depend on the order in which equal
/// distances leave the queue.
pub(crate) fn walk_distance_tree_with(
    net: &StreetNetwork,
    origin: usize,
    limit_mm: i32,
    sc: &mut TreeScratch,
) -> Vec<(u32, i32)> {
    let TreeScratch {
        best_edge,
        touched_edges,
        best_vertex,
        heap,
    } = sc;
    heap.clear();
    // Relax the edges out of `v`, reached by `from_edge` (none at the
    // origin) at distance `d`.
    let mut next = Some((None::<usize>, 0i32, origin));
    while let Some((from_edge, d, v)) = next.take() {
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
            let cur = &mut best_edge[f as usize];
            if d1 < *cur {
                if *cur == i32::MAX {
                    touched_edges.push(f);
                }
                *cur = d1;
                heap.push(d1, f);
            }
        }
        while let Some((d, e)) = heap.pop() {
            if d > best_edge[e as usize] {
                continue;
            }
            next = Some((Some(e as usize), d, net.edge_to(e as usize)));
            break;
        }
    }
    let mut out = Vec::new();
    for &e in touched_edges.iter() {
        let v = net.edge_to(e as usize);
        let d = std::mem::replace(&mut best_edge[e as usize], i32::MAX);
        if best_vertex[v] == i32::MAX {
            out.push((v as u32, 0));
        }
        best_vertex[v] = best_vertex[v].min(d);
    }
    touched_edges.clear();
    for (v, d) in out.iter_mut() {
        *d = std::mem::replace(&mut best_vertex[*v as usize], i32::MAX);
    }
    out.sort_unstable();
    out
}

/// Distance trees (2 km) for every linked stop; `None` for unlinked stops.
pub fn stop_trees(net: &StreetNetwork, layer: &TransitLayer) -> Vec<Option<Vec<(u32, i32)>>> {
    let mut sc = TreeScratch::new(net);
    layer
        .stop_vertex
        .iter()
        .map(|&v| {
            (v >= 0).then(|| walk_distance_tree_with(net, v as usize, EGRESS_LIMIT_MM, &mut sc))
        })
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
    // Position of each stop vertex in that order, and the stop it maps to.
    let max_v = vertex_order.iter().copied().max().unwrap_or(0).max(0) as usize;
    let mut pos_of_vertex = vec![u32::MAX; max_v + 1];
    let order_stop: Vec<u32> = vertex_order
        .iter()
        .enumerate()
        .map(|(k, &v)| {
            pos_of_vertex[v as usize] = k as u32;
            stop_of_vertex[&v]
        })
        .collect();

    let n_patterns = layer.patterns.len();
    // Dense per-stop and per-pattern scratch, reset through touched lists.
    let mut dist = vec![i32::MAX; n];
    let mut best_on_pattern = vec![u32::MAX; n_patterns];
    let mut touched_patterns: Vec<u32> = Vec::new();
    let mut keep = vec![false; n];
    let mut hits: Vec<(u32, i32)> = Vec::new();

    let mut transfers = vec![Vec::new(); n];
    for (s, tree) in trees.iter().enumerate() {
        let Some(tree) = tree else { continue };
        // getReachedStops: a TIntIntHashMap filled in stopForStreetVertex
        // order, i.e. the stop vertices of the tree within the limit, sorted
        // by their position in `vertex_order`.
        hits.clear();
        hits.extend(tree.iter().filter_map(|&(v, d)| {
            if d > TRANSFER_LIMIT_MM {
                return None;
            }
            match pos_of_vertex.get(v as usize) {
                Some(&k) if k != u32::MAX => Some((k, d)),
                _ => None,
            }
        }));
        hits.sort_unstable();
        let mut reached = TroveIntKeys::new();
        for &(k, d) in &hits {
            let stop = order_stop[k as usize];
            reached.insert(stop as i32);
            dist[stop as usize] = d;
        }
        // the source itself is not a transfer
        dist[s] = i32::MAX;
        // retainClosestStopsOnPatterns: first stop seen wins a tie
        let order: Vec<u32> = reached
            .iteration_order()
            .map(|k| k as u32)
            .filter(|&k| dist[k as usize] != i32::MAX)
            .collect();
        for &stop in &order {
            let d = dist[stop as usize];
            for &p in &layer.patterns_for_stop[stop as usize] {
                let cur = &mut best_on_pattern[p as usize];
                if *cur == u32::MAX {
                    touched_patterns.push(p);
                    *cur = stop;
                } else if dist[*cur as usize] > d {
                    *cur = stop;
                }
            }
        }
        for &p in &touched_patterns {
            keep[best_on_pattern[p as usize] as usize] = true;
        }
        transfers[s] = order
            .iter()
            .filter(|&&t| keep[t as usize])
            .map(|&t| (t, dist[t as usize]))
            .collect();
        // reset the scratch
        for &p in &touched_patterns {
            keep[best_on_pattern[p as usize] as usize] = false;
            best_on_pattern[p as usize] = u32::MAX;
        }
        touched_patterns.clear();
        for &(k, _) in &hits {
            dist[order_stop[k as usize] as usize] = i32::MAX;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::street::router::tests::jittered_grid;

    /// The search as R5 defines it, by plain Bellman-Ford relaxation over
    /// directed edges: the reference for [`walk_distance_tree`].
    fn reference_tree(net: &StreetNetwork, origin: usize, limit: i32) -> Vec<(u32, i32)> {
        let mut best = vec![i32::MAX; net.n_edges()];
        let step = |d: i32, f: usize| {
            let mut d1 = d.saturating_add(net.pair_length_mm[f / 2]);
            if d1 == d {
                d1 += 1;
            }
            d1
        };
        for &f in net.out_edges(origin) {
            let f = f as usize;
            if net.edge_flags[f] & ALLOWS_PEDESTRIAN != 0 && step(0, f) <= limit {
                best[f] = best[f].min(step(0, f));
            }
        }
        let mut changed = true;
        while changed {
            changed = false;
            for e in 0..net.n_edges() {
                if best[e] == i32::MAX {
                    continue;
                }
                for &f in net.out_edges(net.edge_to(e)) {
                    let f = f as usize;
                    let flags = net.edge_flags[f];
                    if flags & ALLOWS_PEDESTRIAN == 0
                        || (flags & LINK != 0 && net.edge_flags[e] & LINK != 0)
                    {
                        continue;
                    }
                    let d1 = step(best[e], f);
                    if d1 <= limit && d1 < best[f] {
                        best[f] = d1;
                        changed = true;
                    }
                }
            }
        }
        let mut by_vertex = std::collections::BTreeMap::new();
        for (e, &d) in best.iter().enumerate() {
            if d != i32::MAX {
                let slot = by_vertex.entry(net.edge_to(e) as u32).or_insert(i32::MAX);
                *slot = (*slot).min(d);
            }
        }
        by_vertex.into_iter().collect()
    }

    #[test]
    fn distance_trees_match_the_reference() {
        let net = jittered_grid();
        let mut sc = TreeScratch::new(&net);
        for origin in 0..net.n_vertices() {
            for limit in [500_000, 2_000_000] {
                assert_eq!(
                    walk_distance_tree_with(&net, origin, limit, &mut sc),
                    reference_tree(&net, origin, limit),
                    "origin {origin} limit {limit}"
                );
            }
        }
        assert_eq!(
            walk_distance_tree(&net, 3, 900_000),
            reference_tree(&net, 3, 900_000)
        );
    }

    fn reference_find_transfers(
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

    /// Transfers with dense scratch arrays are those of the hash-map
    /// version they replace (kept above as the reference), tie order
    /// included.
    #[test]
    fn transfers_match_the_hash_map_version() {
        let net = jittered_grid();
        let mut seed = 7u64;
        let mut rnd = |n: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n as u64) as usize
        };
        let n_stops = 90;
        let n_patterns = 25;
        let mut layer = TransitLayer::default();
        for s in 0..n_stops {
            layer.stop_ids.push(format!("f:{s}"));
            // some stops unlinked, some sharing a vertex
            let v = match s % 11 {
                0 => -1,
                1 => layer.stop_vertex.last().copied().unwrap_or(-1),
                _ => rnd(net.n_vertices()) as i32,
            };
            layer.stop_vertex.push(v);
            layer
                .patterns_for_stop
                .push((0..rnd(4)).map(|_| rnd(n_patterns) as u32).collect());
        }
        layer.patterns = vec![Default::default(); n_patterns];
        let trees: Vec<_> = layer
            .stop_vertex
            .iter()
            .map(|&v| (v >= 0).then(|| walk_distance_tree(&net, v as usize, EGRESS_LIMIT_MM)))
            .collect();
        let got = find_transfers(&layer, &trees);
        assert_eq!(got, reference_find_transfers(&layer, &trees));
        assert!(got.iter().map(Vec::len).sum::<usize>() > 100);
    }
}
