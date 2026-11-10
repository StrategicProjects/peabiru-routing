//! Building a [`StreetNetwork`] from an OpenStreetMap PBF extract, following
//! R5 7.6 `StreetLayer.loadFromOsm`.

use super::flags::*;
use super::geom::{
    first_last_angles, haversine_m, node_fixed, radians_to_brads, vertex_fixed, FIXED_FACTOR,
};
use super::labels::{
    car_speed_ms, is_way_routable, label_lts, linkable, lts_of, permissions, speed_to_cms, tag,
};
use super::vertex_flags::*;
use super::{link, StreetNetwork};
use crate::osm::pbf::{read_pbf_file, Extract, PbfError, ReadOptions, Way};

/// Smallest strongly connected component kept per mode (R5 `MIN_SUBGRAPH_SIZE`).
pub const MIN_SUBGRAPH_SIZE: usize = 40;

/// Options for [`build_network`].
#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    /// Worker threads for reading the PBF; 0 means all cores, and more
    /// than the cores available are not used.
    pub n_threads: usize,
}

/// Read a PBF file and build the street network.
///
/// A file that gives no street edge at all (empty, a header only, or no
/// routable way) is an error naming the file. R5 fails on such a file too,
/// with an `ArrayIndexOutOfBoundsException` from `StreetLayer.loadFromOsm`.
pub fn build_network(
    path: impl AsRef<std::path::Path>,
    opts: &BuildOptions,
) -> Result<StreetNetwork, PbfError> {
    let path = path.as_ref();
    let not_building = |t: &[(&str, &str)]| !t.iter().any(|(k, _)| *k == "building");
    let node_tags = |t: &[(&str, &str)]| {
        t.iter()
            .any(|(k, _)| matches!(*k, "entrance" | "access" | "highway"))
    };
    let park_ride = |t: &[(&str, &str)]| {
        t.iter()
            .any(|(k, v)| *k == "park_ride" && super::parkride::is_park_and_ride_value(v))
    };
    // Routable ways become edges; park-and-ride ways (routable or not) are
    // linked to the streets afterwards.
    let ways = |t: &[(&str, &str)]| is_way_routable(t) || park_ride(t);
    let read = ReadOptions {
        way_filter: &ways,
        relation_filter: Some(&super::restrictions::is_restriction),
        node_tag_filter: Some(&node_tags),
        ref_count_filter: Some(&not_building),
        poi_filter: Some(&park_ride),
        n_threads: opts.n_threads,
    };
    let ex = read_pbf_file(path, &read)?;
    let net = network_from_extract(ex);
    // Connectors (park-and-ride links) are the only pairs that do not come
    // from a way, and splits only exist where a street pair does.
    if net.edge_flags.iter().step_by(2).all(|&f| f & LINK != 0) {
        return Err(PbfError::InFile {
            path: path.to_path_buf(),
            error: Box::new(PbfError::NoStreets),
        });
    }
    Ok(net)
}

/// References to each extract node from the extract's ways without a
/// `building` tag, repeats within a way included (what the reader counts
/// with [`build_network`]'s `ref_count_filter`).
fn ref_counts_of_ways(ex: &Extract) -> Vec<u32> {
    let mut counts = vec![0u32; ex.node_ids.len()];
    for w in &ex.ways {
        if w.tags.0.iter().any(|(k, _)| k == "building") {
            continue;
        }
        for &id in &w.nodes {
            if let Some(i) = ex.node_index(id) {
                counts[i] = counts[i].saturating_add(1);
            }
        }
    }
    counts
}

/// Builder state while ways are turned into edge pairs.
struct Builder<'a> {
    ex: &'a Extract,
    /// R5 fixed coordinates of every extract node.
    node_lat: Vec<i32>,
    node_lon: Vec<i32>,
    /// Vertex currently mapped to each extract node, or `u32::MAX`.
    vertex_of: Vec<u32>,
    impassable: Vec<bool>,
    signal: Vec<bool>,
    net: StreetNetwork,
}

/// Build the network from an already-read extract. For R5's network the
/// extract must come from [`build_network`]'s read options (routable ways,
/// reference counts over non-building ways, node tags).
///
/// An extract read without reference counts (`ref_count_filter: None`) gets
/// them from its own non-building ways; ways the read filtered out then do
/// not count, so a node shared only with such a way is not an intersection.
/// A node without a latitude or longitude (`lat` or `lon` shorter than
/// `node_ids`) is treated as absent from the file, like `i32::MIN`.
pub fn network_from_extract(mut ex: Extract) -> StreetNetwork {
    let n = ex.node_ids.len();
    if ex.lat.len() != n || ex.lon.len() != n {
        ex.lat.resize(n, i32::MIN);
        ex.lon.resize(n, i32::MIN);
        for i in 0..n {
            if ex.lat[i] == i32::MIN || ex.lon[i] == i32::MIN {
                (ex.lat[i], ex.lon[i]) = (i32::MIN, i32::MIN);
            }
        }
    }
    if ex.ref_counts.len() != n {
        ex.ref_counts = ref_counts_of_ways(&ex);
    }
    ex.ways.sort_by_key(|w| w.id); // R5 iterates ways in ascending id
    let mut impassable = vec![false; n];
    let mut signal = vec![false; n];
    for (id, tags) in &ex.node_tags {
        if let Some(i) = ex.node_index(*id) {
            let t = &tags.0;
            signal[i] = tag(t, "highway") == Some("traffic_signals");
            impassable[i] = is_impassable(t);
        }
    }
    let node_lat = ex
        .lat
        .iter()
        .map(|&v| if v == i32::MIN { v } else { node_fixed(v) })
        .collect();
    let node_lon = ex
        .lon
        .iter()
        .map(|&v| if v == i32::MIN { v } else { node_fixed(v) })
        .collect();
    let mut b = Builder {
        ex: &ex,
        node_lat,
        node_lon,
        vertex_of: vec![u32::MAX; n],
        impassable,
        signal,
        net: StreetNetwork::default(),
    };
    fn borrowed(w: &Way) -> Vec<(&str, &str)> {
        w.tags
            .0
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }
    for w in &ex.ways {
        if is_way_routable(&borrowed(w)) {
            b.add_way(w);
        }
    }
    let vertex_of = std::mem::take(&mut b.vertex_of);
    let (node_lat, node_lon) = (
        std::mem::take(&mut b.node_lat),
        std::mem::take(&mut b.node_lon),
    );
    let mut net = b.net;
    build_adjacency(&mut net);
    apply_intersection_lts(&mut net);
    for (mode_flag, walk_counts) in [
        (ALLOWS_CAR, false),
        (ALLOWS_PEDESTRIAN, false),
        (ALLOWS_BIKE, true),
    ] {
        prune_islands(&mut net, mode_flag, walk_counts);
    }
    net.index = link::EdgeIndex::build(&net);

    // Park-and-ride: areas (ways, in id order) then nodes (in id order).
    let is_pr_way = |w: &Way| {
        w.tags
            .get("park_ride")
            .is_some_and(super::parkride::is_park_and_ride_value)
    };
    let areas: Vec<Vec<(i32, i32)>> = ex
        .ways
        .iter()
        .filter(|w| is_pr_way(w))
        .filter_map(|w| {
            // R5 cannot handle a park-and-ride way with a missing node; skip it
            w.nodes
                .iter()
                .map(|&id| {
                    ex.node_index(id)
                        .filter(|&i| node_lat[i] != i32::MIN)
                        .map(|i| (node_lat[i], node_lon[i]))
                })
                .collect::<Option<Vec<_>>>()
        })
        .collect();
    let pr_nodes: Vec<(f64, f64)> = ex
        .pois
        .iter()
        .map(|&(_, lat, lon, _)| {
            (
                f64::from(node_fixed(lat)) / FIXED_FACTOR,
                f64::from(node_fixed(lon)) / FIXED_FACTOR,
            )
        })
        .collect();
    super::parkride::apply_park_and_ride(&mut net, &areas, &pr_nodes);

    // R5 builds turn restrictions with the edge lists from before the
    // park-and-ride splits (they are rebuilt only afterwards), so do the same.
    let vertex_for_node = |id: i64| {
        ex.node_index(id)
            .map(|i| vertex_of[i])
            .filter(|&v| v != u32::MAX)
    };
    super::restrictions::apply_turn_restrictions(&mut net, &ex, &vertex_for_node);
    build_adjacency(&mut net);
    net
}

/// R5 `isImpassable` (step-free routing off).
fn is_impassable(t: &[(String, String)]) -> bool {
    if t.is_empty() {
        return false;
    }
    if tag(t, "entrance") == Some("emergency") {
        return true;
    }
    if tag(t, "access") == Some("no") {
        let null_or_no = |k: &str| matches!(tag(t, k), None | Some("no"));
        return null_or_no("foot") && null_or_no("bicycle");
    }
    false
}

impl Builder<'_> {
    fn node(&self, id: i64) -> Option<usize> {
        self.ex
            .node_index(id)
            .filter(|&i| self.node_lat[i] != i32::MIN)
    }

    /// R5 `getVertexIndexForOsmNode`: one vertex per node, except that an
    /// impassable node gets a fresh vertex at every reference.
    fn vertex(&mut self, i: usize) -> u32 {
        let v = self.vertex_of[i];
        if v != u32::MAX && self.net.v_flags[v as usize] & IMPASSABLE == 0 {
            return v;
        }
        let v = self.net.v_lat.len() as u32;
        self.net.v_lat.push(vertex_fixed(self.node_lat[i]));
        self.net.v_lon.push(vertex_fixed(self.node_lon[i]));
        let mut f = 0;
        if self.signal[i] {
            f |= TRAFFIC_SIGNAL;
        }
        if self.impassable[i] {
            f |= IMPASSABLE;
        }
        self.net.v_flags.push(f);
        self.vertex_of[i] = v;
        v
    }

    fn add_way(&mut self, w: &Way) {
        let mut begin = 0usize;
        for n in 1..w.nodes.len() {
            let Some(i) = self.node(w.nodes[n]) else {
                break; // R5 bails out of a way with an undefined node
            };
            let intersection = self.ex.ref_counts[i] >= 2;
            let last = n == w.nodes.len() - 1;
            if intersection || last || self.impassable[i] {
                self.make_edge_pair(w, begin, n);
                begin = n;
            }
        }
    }

    fn make_edge_pair(&mut self, w: &Way, begin: usize, end: usize) {
        // Vertices are created first, even if the segment is dropped below.
        let (n0, n1) = (self.node(w.nodes[begin]), self.node(w.nodes[end]));
        let (Some(i0), Some(i1)) = (n0, n1) else {
            for i in [n0, n1].into_iter().flatten() {
                self.vertex(i);
            }
            return;
        };
        let v0 = self.vertex(i0);
        let v1 = self.vertex(i1);
        let mut idx = Vec::with_capacity(end - begin + 1);
        for &id in &w.nodes[begin..=end] {
            match self.node(id) {
                Some(i) => idx.push(i),
                None => return,
            }
        }
        let deg = |fixed: i32| f64::from(fixed) / FIXED_FACTOR;
        let mut length_m = 0.0;
        for p in idx.windows(2) {
            length_m += haversine_m(
                deg(self.node_lat[p[0]]),
                deg(self.node_lon[p[0]]),
                deg(self.node_lat[p[1]]),
                deg(self.node_lon[p[1]]),
            );
        }
        if length_m * 1000.0 > f64::from(i32::MAX) {
            return;
        }
        let length_mm = (length_m * 1000.0) as i32;

        let tags = &w.tags.0;
        let fwd_speed = speed_to_cms(car_speed_ms(tags, false));
        let bwd_speed = speed_to_cms(car_speed_ms(tags, true));
        let (mut fwd, mut bwd) = permissions(tags);
        if (fwd | bwd) & ALL_PERMISSIONS == 0 {
            return;
        }
        label_lts(tags, &mut fwd, &mut bwd);
        if linkable(tags) {
            fwd |= LINKABLE;
            bwd |= LINKABLE;
        }

        let net = &mut self.net;
        net.pair_from.push(v0);
        net.pair_to.push(v1);
        net.pair_length_mm.push(length_mm);
        net.pair_way.push(w.id);
        let start = net.geom.len() as u32;
        for &i in &idx[1..idx.len() - 1] {
            net.geom.push((self.node_lat[i], self.node_lon[i]));
        }
        net.pair_geom.push((start, net.geom.len() as u32));

        // Angles use the vertex coordinates at the ends and the node
        // coordinates in between (R5 getGeometry), as (lon, lat) degrees.
        let mut pts = Vec::with_capacity(idx.len());
        pts.push((deg(net.v_lon[v0 as usize]), deg(net.v_lat[v0 as usize])));
        for &i in &idx[1..idx.len() - 1] {
            pts.push((deg(self.node_lon[i]), deg(self.node_lat[i])));
        }
        pts.push((deg(net.v_lon[v1 as usize]), deg(net.v_lat[v1 as usize])));
        let (first, last) = first_last_angles(&pts);
        net.pair_in_brad.push(radians_to_brads(first));
        net.pair_out_brad.push(radians_to_brads(last));

        net.edge_flags.push(fwd);
        net.edge_flags.push(bwd);
        net.edge_speed_cms.push(fwd_speed);
        net.edge_speed_cms.push(bwd_speed);
    }
}

pub(crate) fn build_adjacency(net: &mut StreetNetwork) {
    let nv = net.n_vertices();
    let ne = net.n_edges();
    let mut out_deg = vec![0u32; nv + 1];
    let mut in_deg = vec![0u32; nv + 1];
    for e in 0..ne {
        out_deg[net.edge_from(e) + 1] += 1;
        in_deg[net.edge_to(e) + 1] += 1;
    }
    for v in 0..nv {
        out_deg[v + 1] += out_deg[v];
        in_deg[v + 1] += in_deg[v];
    }
    let mut out_edges = vec![0u32; ne];
    let mut in_edges = vec![0u32; ne];
    let mut oc = out_deg.clone();
    let mut ic = in_deg.clone();
    for e in 0..ne {
        let f = net.edge_from(e);
        out_edges[oc[f] as usize] = e as u32;
        oc[f] += 1;
        let t = net.edge_to(e);
        in_edges[ic[t] as usize] = e as u32;
        ic[t] += 1;
    }
    net.out_offsets = out_deg;
    net.out_edges = out_edges;
    net.in_offsets = in_deg;
    net.in_edges = in_edges;
}

/// Spread the highest LTS at each unsignalled vertex to every edge touching
/// it (R5 `applyIntersectionCosts`).
fn apply_intersection_lts(net: &mut StreetNetwork) {
    let nv = net.n_vertices();
    let mut stress = vec![0i32; nv]; // 0 = signalled, skipped
    for (v, s) in stress.iter_mut().enumerate() {
        if net.v_flags[v] & TRAFFIC_SIGNAL != 0 {
            continue;
        }
        let mut max_lts = 1;
        for &e in net.in_edges(v).iter().chain(net.out_edges(v)) {
            let f = net.edge_flags[e as usize];
            if f & BIKE_LTS_2 != 0 {
                max_lts = max_lts.max(2);
            }
            if f & BIKE_LTS_3 != 0 {
                max_lts = max_lts.max(3);
            }
            if f & BIKE_LTS_4 != 0 {
                max_lts = max_lts.max(4);
            }
        }
        *s = max_lts;
    }
    for (v, &s) in stress.iter().enumerate() {
        if s == 0 {
            continue;
        }
        let incident: Vec<u32> = net
            .in_edges(v)
            .iter()
            .chain(net.out_edges(v))
            .copied()
            .collect();
        for e in incident {
            let f = &mut net.edge_flags[e as usize];
            if *f & BIKE_LTS_EXPLICIT != 0 {
                continue;
            }
            let own = lts_of(*f & (BIKE_LTS_2 | BIKE_LTS_3 | BIKE_LTS_4));
            let lts = s.max(own);
            *f &= !LTS_MASK;
            *f |= match lts {
                1 => BIKE_LTS_1,
                2 => BIKE_LTS_2,
                3 => BIKE_LTS_3,
                _ => BIKE_LTS_4,
            };
        }
    }
}

/// Remove `mode_flag` around every vertex in a strongly connected component
/// smaller than [`MIN_SUBGRAPH_SIZE`] (R5 `TarjanIslandPruner`). For bikes the
/// component graph also follows walkable edges (`walk_counts`).
fn prune_islands(net: &mut StreetNetwork, mode_flag: u32, walk_counts: bool) {
    let usable = |f: u32| f & mode_flag != 0 || (walk_counts && f & ALLOWS_PEDESTRIAN != 0);
    let comp = strongly_connected_components(net, &usable);
    let mut size = vec![0usize; comp.iter().copied().max().map_or(0, |m| m as usize + 1)];
    for &c in &comp {
        size[c as usize] += 1;
    }
    for v in 0..net.n_vertices() {
        if size[comp[v] as usize] < MIN_SUBGRAPH_SIZE {
            let incident: Vec<u32> = net
                .out_edges(v)
                .iter()
                .chain(net.in_edges(v))
                .copied()
                .collect();
            for e in incident {
                net.edge_flags[e as usize] &= !mode_flag;
            }
        }
    }
}

/// Iterative Tarjan: component id of every vertex over the edges `usable`
/// accepts.
fn strongly_connected_components(net: &StreetNetwork, usable: &dyn Fn(u32) -> bool) -> Vec<u32> {
    const UNVISITED: u32 = u32::MAX;
    let nv = net.n_vertices();
    let mut index = vec![UNVISITED; nv];
    let mut low = vec![0u32; nv];
    let mut on_stack = vec![false; nv];
    let mut comp = vec![UNVISITED; nv];
    let mut stack: Vec<u32> = Vec::new();
    let mut next_index = 0u32;
    let mut next_comp = 0u32;
    // call stack of (vertex, position in its out-edge list)
    let mut calls: Vec<(u32, u32)> = Vec::new();
    for root in 0..nv {
        if index[root] != UNVISITED {
            continue;
        }
        calls.push((root as u32, 0));
        index[root] = next_index;
        low[root] = next_index;
        next_index += 1;
        stack.push(root as u32);
        on_stack[root] = true;
        while let Some(&(v, pos0)) = calls.last() {
            let v = v as usize;
            let edges = net.out_edges(v);
            let mut pos = pos0 as usize;
            let mut child = None;
            while pos < edges.len() {
                let e = edges[pos] as usize;
                pos += 1;
                if !usable(net.edge_flags[e]) {
                    continue;
                }
                let w = net.edge_to(e);
                if index[w] == UNVISITED {
                    child = Some(w);
                    break;
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            }
            calls.last_mut().expect("call stack").1 = pos as u32;
            if let Some(w) = child {
                index[w] = next_index;
                low[w] = next_index;
                next_index += 1;
                stack.push(w as u32);
                on_stack[w] = true;
                calls.push((w as u32, 0));
                continue;
            }
            calls.pop();
            if let Some(&(p, _)) = calls.last() {
                let p = p as usize;
                low[p] = low[p].min(low[v]);
            }
            if low[v] == index[v] {
                loop {
                    let w = stack.pop().expect("Tarjan stack underflow") as usize;
                    on_stack[w] = false;
                    comp[w] = next_comp;
                    if w == v {
                        break;
                    }
                }
                next_comp += 1;
            }
        }
    }
    comp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::street::link::tests::{jittered_grid, Rng};

    /// An extract read without reference counts builds, with the counts of
    /// its own ways; on a network where every shared node is shared by kept
    /// ways that is the same network.
    #[test]
    fn extract_without_ref_counts_gets_them_from_its_ways() {
        let ex = jittered_grid(&mut Rng(7), 6, -23.5, -46.6, 0.003);
        let mut bare = ex.clone();
        bare.ref_counts.clear();
        let (a, b) = (network_from_extract(ex), network_from_extract(bare));
        assert!(a.pair_from.len() > 50);
        assert_eq!(a.pair_from, b.pair_from);
        assert_eq!(a.pair_to, b.pair_to);
        assert_eq!(a.pair_length_mm, b.pair_length_mm);
        assert_eq!(a.geom, b.geom);
    }

    /// Extracts built by hand do not make the public builder panic: nodes
    /// without coordinates count as absent from the file, and other
    /// inconsistencies give some network rather than a crash.
    #[test]
    fn inconsistent_extracts_do_not_panic() {
        use crate::osm::pbf::{Member, MemberKind, Relation, Tags};
        let ex = jittered_grid(&mut Rng(3), 5, -23.5, -46.6, 0.003);
        let n = ex.node_ids.len();
        let full = network_from_extract(ex.clone());

        // Coordinates of the last node missing: same as marking it absent.
        let mut short = ex.clone();
        short.lat.truncate(n - 1);
        short.lon.pop();
        short.lon.pop();
        let mut absent = ex.clone();
        for i in [n - 2, n - 1] {
            (absent.lat[i], absent.lon[i]) = (i32::MIN, i32::MIN);
        }
        let (a, b) = (network_from_extract(short), network_from_extract(absent));
        assert_eq!(a.pair_from, b.pair_from);
        assert_eq!(a.pair_length_mm, b.pair_length_mm);
        assert!(a.pair_from.len() < full.pair_from.len());

        // No coordinates at all, unsorted and repeated ids, odd ways and
        // relations.
        let mut bare = ex.clone();
        bare.lat.clear();
        bare.lon.clear();
        assert_eq!(network_from_extract(bare).n_edges(), 0);
        let mut odd = ex.clone();
        odd.node_ids.reverse();
        odd.node_ids[0] = odd.node_ids[1];
        odd.ref_counts.truncate(3);
        let highway = || Tags(vec![("highway".into(), "residential".into())]);
        odd.ways.push(Way {
            id: 1 << 40,
            tags: highway(),
            nodes: vec![],
        });
        odd.ways.push(Way {
            id: (1 << 40) + 1,
            tags: highway(),
            nodes: vec![1, 1, 1, 999_999],
        });
        odd.relations.push(Relation {
            id: 7,
            tags: Tags(vec![
                ("type".into(), "restriction".into()),
                ("restriction".into(), "no_left_turn".into()),
            ]),
            members: vec![
                Member {
                    kind: MemberKind::Way,
                    id: 1 << 40,
                    role: "from".into(),
                },
                Member {
                    kind: MemberKind::Node,
                    id: 1,
                    role: "via".into(),
                },
                Member {
                    kind: MemberKind::Way,
                    id: 424_242,
                    role: "to".into(),
                },
            ],
        });
        odd.member_ways_present = vec![1 << 40];
        odd.member_nodes_present = vec![1];
        network_from_extract(odd);
    }
}
