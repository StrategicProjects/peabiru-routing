//! Linking points to the street network (R5 `Split.find`).

use super::flags::*;
use super::geom::{java_int, segment_fraction, FIXED_FACTOR};
use super::StreetNetwork;
use std::collections::HashMap;

/// First and fallback search radii in metres (R5 `INITIAL_LINK_RADIUS_METERS`,
/// `LINK_RADIUS_METERS`).
pub const INITIAL_LINK_RADIUS_M: f64 = 300.0;
pub const LINK_RADIUS_M: f64 = 1600.0;
const METERS_PER_DEGREE_LAT: f64 = 111_111.111;

/// Where a point meets the network: a position along a forward edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Split {
    /// Forward (even) edge index.
    pub edge: u32,
    /// From and to vertex of that edge.
    pub vertex0: u32,
    pub vertex1: u32,
    /// Distance along the edge from `vertex0` and to `vertex1`, millimetres.
    pub distance0_mm: i32,
    pub distance1_mm: i32,
    /// Off-street distance from the point to the edge, millimetres.
    pub distance_to_edge_mm: i32,
    /// Segment of the edge geometry holding the split, and the split point
    /// (fixed-point), used when a split cuts the edge in two.
    pub seg: u32,
    pub fixed_lat: i32,
    pub fixed_lon: i32,
}

/// Uniform grid over forward-edge bounding boxes, in fixed-point degrees,
/// storing only the cells that hold a pair (memory follows the network, not
/// the area of its bounding box).
#[derive(Debug, Clone, Default)]
pub struct EdgeIndex {
    min_lat: i32,
    min_lon: i32,
    n_rows: usize,
    n_cols: usize,
    /// Occupied cells, as `row * n_cols + col`, ascending.
    cells: Vec<u64>,
    /// Pairs in `cells[i]`: `pairs[offsets[i]..offsets[i + 1]]`.
    offsets: Vec<u32>,
    pairs: Vec<u32>,
    /// Pairs added after the grid was built (edge splits), by cell (row and
    /// column on the same grid, unbounded).
    extra: HashMap<(i64, i64), Vec<u32>>,
}

/// Cell side in fixed-point degrees (0.005 degrees, about 550 m).
const CELL: i64 = 50_000;

/// Margin around a query envelope when looking up split pairs. A pair can
/// be within the link radius of a point and still lie a few fixed-point
/// units outside the query envelope, whose longitude half-width is
/// truncated to an integer and whose distances are measured after
/// truncating the scaled longitude difference (the overshoot is below
/// `2 + 1 / cos(lat)` units; one cell covers it up to 89.998 degrees).
/// The margin keeps every split pair a query could select.
const EXTRA_MARGIN: i64 = CELL;

/// Bounding box `(min_lat, min_lon, max_lat, max_lon)` of pair `k`.
pub fn pair_bbox(net: &StreetNetwork, k: usize) -> (i32, i32, i32, i32) {
    let mut b = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for (lat, lon) in net.pair_points(k) {
        b = (b.0.min(lat), b.1.min(lon), b.2.max(lat), b.3.max(lon));
    }
    b
}

impl EdgeIndex {
    pub fn build(net: &StreetNetwork) -> Self {
        let n_pairs = net.pair_from.len();
        if n_pairs == 0 {
            return EdgeIndex::default();
        }
        let boxes: Vec<_> = (0..n_pairs).map(|k| pair_bbox(net, k)).collect();
        let min_lat = boxes.iter().map(|b| b.0).min().unwrap();
        let min_lon = boxes.iter().map(|b| b.1).min().unwrap();
        let max_lat = boxes.iter().map(|b| b.2).max().unwrap();
        let max_lon = boxes.iter().map(|b| b.3).max().unwrap();
        let n_rows = ((i64::from(max_lat) - i64::from(min_lat)) / CELL + 1) as usize;
        let n_cols = ((i64::from(max_lon) - i64::from(min_lon)) / CELL + 1) as usize;
        let cell_range = |b: &(i32, i32, i32, i32)| {
            let r0 = ((i64::from(b.0) - i64::from(min_lat)) / CELL) as u64;
            let r1 = ((i64::from(b.2) - i64::from(min_lat)) / CELL) as u64;
            let c0 = ((i64::from(b.1) - i64::from(min_lon)) / CELL) as u64;
            let c1 = ((i64::from(b.3) - i64::from(min_lon)) / CELL) as u64;
            (r0..=r1).flat_map(move |r| (c0..=c1).map(move |c| r * n_cols as u64 + c))
        };
        let mut members: Vec<(u64, u32)> = Vec::new();
        for (k, b) in boxes.iter().enumerate() {
            members.extend(cell_range(b).map(|c| (c, k as u32)));
        }
        members.sort_unstable();
        let mut idx = EdgeIndex {
            min_lat,
            min_lon,
            n_rows,
            n_cols,
            cells: Vec::new(),
            offsets: Vec::new(),
            pairs: Vec::with_capacity(members.len()),
            extra: HashMap::new(),
        };
        for (c, k) in members {
            if idx.cells.last() != Some(&c) {
                idx.cells.push(c);
                idx.offsets.push(idx.pairs.len() as u32);
            }
            idx.pairs.push(k);
        }
        idx.offsets.push(idx.pairs.len() as u32);
        idx
    }

    /// Grid row and column of a fixed-point position (unbounded).
    fn cell_of(&self, lat: i64, lon: i64) -> (i64, i64) {
        (
            (lat - i64::from(self.min_lat)).div_euclid(CELL),
            (lon - i64::from(self.min_lon)).div_euclid(CELL),
        )
    }

    /// Register a pair created after the grid was built, with its bounding
    /// box. The pair's geometry must stay inside that box (later splits of
    /// the pair cut it at points on its geometry, so it does).
    pub fn insert_extra(&mut self, pair: u32, bbox: (i32, i32, i32, i32)) {
        let (r0, c0) = self.cell_of(i64::from(bbox.0), i64::from(bbox.1));
        let (r1, c1) = self.cell_of(i64::from(bbox.2), i64::from(bbox.3));
        for r in r0..=r1 {
            for c in c0..=c1 {
                self.extra.entry((r, c)).or_default().push(pair);
            }
        }
    }

    /// Pairs whose cells overlap an envelope, fixed-point, in no particular
    /// order and with repeats: every pair whose bounding box meets the
    /// envelope, and possibly others. Split pairs are looked up with
    /// [`EXTRA_MARGIN`] around the envelope.
    pub fn query(
        &self,
        min_lat: i64,
        min_lon: i64,
        max_lat: i64,
        max_lon: i64,
        out: &mut Vec<u32>,
    ) {
        out.clear();
        if !self.extra.is_empty() {
            let (r0, c0) = self.cell_of(min_lat - EXTRA_MARGIN, min_lon - EXTRA_MARGIN);
            let (r1, c1) = self.cell_of(max_lat + EXTRA_MARGIN, max_lon + EXTRA_MARGIN);
            let n_cells = (r1 - r0 + 1).saturating_mul(c1 - c0 + 1);
            if n_cells > self.extra.len() as i64 {
                for (&(r, c), pairs) in &self.extra {
                    if (r0..=r1).contains(&r) && (c0..=c1).contains(&c) {
                        out.extend_from_slice(pairs);
                    }
                }
            } else {
                for r in r0..=r1 {
                    for c in c0..=c1 {
                        if let Some(pairs) = self.extra.get(&(r, c)) {
                            out.extend_from_slice(pairs);
                        }
                    }
                }
            }
        }
        if self.n_rows == 0 {
            return;
        }
        let clamp_row = |lat: i64| {
            ((lat - i64::from(self.min_lat)).div_euclid(CELL)).clamp(0, self.n_rows as i64 - 1)
                as usize
        };
        let clamp_col = |lon: i64| {
            ((lon - i64::from(self.min_lon)).div_euclid(CELL)).clamp(0, self.n_cols as i64 - 1)
                as usize
        };
        if max_lat < i64::from(self.min_lat) || max_lon < i64::from(self.min_lon) {
            return;
        }
        let max_lat_grid = i64::from(self.min_lat) + self.n_rows as i64 * CELL;
        let max_lon_grid = i64::from(self.min_lon) + self.n_cols as i64 * CELL;
        if min_lat >= max_lat_grid || min_lon >= max_lon_grid {
            return;
        }
        let (c0, c1) = (clamp_col(min_lon) as u64, clamp_col(max_lon) as u64);
        for r in clamp_row(min_lat)..=clamp_row(max_lat) {
            let row = r as u64 * self.n_cols as u64;
            let mut i = self.cells.partition_point(|&cell| cell < row + c0);
            while i < self.cells.len() && self.cells[i] <= row + c1 {
                out.extend_from_slice(
                    &self.pairs[self.offsets[i] as usize..self.offsets[i + 1] as usize],
                );
                i += 1;
            }
        }
    }
}

/// Edge flag a mode needs to link (R5 `allowsStreetMode`).
pub fn link_flag(mode: super::Mode) -> u32 {
    match mode {
        super::Mode::Walk => ALLOWS_PEDESTRIAN,
        super::Mode::Bicycle => ALLOWS_BIKE,
        super::Mode::Car => ALLOWS_CAR,
    }
}

/// Link a point to the nearest eligible edge, first within 300 m then within
/// 1600 m (R5 `StreetLayer.findSplit`). `scratch` is a reusable buffer.
pub fn find_split(
    net: &StreetNetwork,
    lat: f64,
    lon: f64,
    mode: super::Mode,
    scratch: &mut Vec<u32>,
) -> Option<Split> {
    find_split_radius(net, lat, lon, INITIAL_LINK_RADIUS_M, mode, scratch)
        .or_else(|| find_split_radius(net, lat, lon, LINK_RADIUS_M, mode, scratch))
}

/// Points linked per batch by [`link_points`].
const LINK_BATCH: usize = 512;

/// [`find_split`] for `n` points, `point(i)` giving point `i` as `(lat,
/// lon)`, shared out in batches among up to `n_threads` threads (0 = all
/// cores): the splits in point order, the same whatever the number of
/// threads. `None` when `cancel` is cancelled, which the workers check
/// between batches.
pub(crate) fn link_points(
    net: &StreetNetwork,
    n: usize,
    point: impl Fn(usize) -> (f64, f64) + Sync,
    mode: super::Mode,
    n_threads: usize,
    cancel: &crate::CancelToken,
) -> Option<Vec<Option<Split>>> {
    let mut out = vec![None; n];
    let n_threads = super::router::worker_threads(n_threads, n.div_ceil(LINK_BATCH));
    let batches = std::sync::Mutex::new(out.chunks_mut(LINK_BATCH).enumerate());
    crate::sys::scoped(0..n_threads, |_| {
        let mut scratch = Vec::new();
        while !cancel.is_cancelled() {
            // nothing panics while the lock is held
            let next = batches.lock().unwrap_or_else(|e| e.into_inner()).next();
            let Some((b, slots)) = next else { break };
            for (k, slot) in slots.iter_mut().enumerate() {
                let (lat, lon) = point(b * LINK_BATCH + k);
                *slot = find_split(net, lat, lon, mode, &mut scratch);
            }
        }
    });
    (!cancel.is_cancelled()).then_some(out)
}

/// R5 `Split.find` within one radius.
pub fn find_split_radius(
    net: &StreetNetwork,
    lat: f64,
    lon: f64,
    radius_m: f64,
    mode: super::Mode,
    scratch: &mut Vec<u32>,
) -> Option<Split> {
    let fixed_lat = java_int(lat * FIXED_FACTOR);
    let fixed_lon = java_int(lon * FIXED_FACTOR);
    let cos_lat = lat.to_radians().cos();
    let radius_lat = i64::from(java_int(radius_m / METERS_PER_DEGREE_LAT * FIXED_FACTOR));
    let radius_lon = i64::from(java_int(radius_lat as f64 / cos_lat));
    net.index.query(
        i64::from(fixed_lat) - radius_lat,
        i64::from(fixed_lon) - radius_lon,
        i64::from(fixed_lat) + radius_lat,
        i64::from(fixed_lon) + radius_lon,
        scratch,
    );
    scratch.sort_unstable();
    scratch.dedup();
    select_split(net, lat, lon, radius_m, mode, scratch)
}

/// The nearest eligible pair among `candidates` within the radius, ties to
/// the lowest pair. The result does not depend on the order of the
/// candidates, nor on extra candidates beyond the radius.
fn select_split(
    net: &StreetNetwork,
    lat: f64,
    lon: f64,
    radius_m: f64,
    mode: super::Mode,
    candidates: &[u32],
) -> Option<Split> {
    let fixed_lat = java_int(lat * FIXED_FACTOR);
    let fixed_lon = java_int(lon * FIXED_FACTOR);
    let cos_lat = lat.to_radians().cos();
    let radius_lat = i64::from(java_int(radius_m / METERS_PER_DEGREE_LAT * FIXED_FACTOR));
    let sq_radius = radius_lat * radius_lat;
    let flag = link_flag(mode) | LINKABLE;

    // best: (squared distance, pair, segment, fraction, split lat, split lon)
    let mut best: Option<(i64, usize, usize, f64, i32, i32)> = None;
    for &k in candidates {
        let k = k as usize;
        if net.edge_flags[2 * k] & flag != flag || net.edge_flags[2 * k + 1] & flag != flag {
            continue;
        }
        let mut prev: Option<(i32, i32)> = None;
        for (seg_plus_one, (plat, plon)) in net.pair_points(k).enumerate() {
            let Some((lat0, lon0)) = prev.replace((plat, plon)) else {
                continue;
            };
            let seg = seg_plus_one - 1;
            let (lat1, lon1) = (plat, plon);
            let frac = segment_fraction(lon0, lat0, lon1, lat1, fixed_lon, fixed_lat, cos_lat);
            let p_lon = java_int(f64::from(lon0) + frac * f64::from(lon1 - lon0));
            let p_lat = java_int(f64::from(lat0) + frac * f64::from(lat1 - lat0));
            let dx = (f64::from(p_lon.wrapping_sub(fixed_lon)) * cos_lat) as i64;
            let dy = i64::from(p_lat) - i64::from(fixed_lat);
            let d2 = dx * dx + dy * dy;
            if d2 < sq_radius {
                let better = match best {
                    None => true,
                    Some((bd2, bk, ..)) => d2 < bd2 || (d2 == bd2 && k < bk),
                };
                if better {
                    best = Some((d2, k, seg, frac, p_lat, p_lon));
                }
            }
        }
    }
    let (d2, k, best_seg, best_frac, split_lat, split_lon) = best?;

    // Length along the edge up to the split, planar at the point's latitude.
    let mut before = 0.0f64;
    let mut prev: Option<(i32, i32)> = None;
    for (seg_plus_one, (plat, plon)) in net.pair_points(k).enumerate() {
        let Some((lat0, lon0)) = prev.replace((plat, plon)) else {
            continue;
        };
        let seg = seg_plus_one - 1;
        if seg <= best_seg {
            let dx = f64::from(plon - lon0) * cos_lat;
            let dy = f64::from(plat - lat0);
            let mut len = (dx * dx + dy * dy).sqrt();
            if seg == best_seg {
                len *= best_frac;
            }
            before += len;
        }
    }
    let length_mm = net.pair_length_mm[k];
    let before_deg = f64::from(java_int(before)) / FIXED_FACTOR;
    let d0 = java_int(before_deg * METERS_PER_DEGREE_LAT * 1000.0).clamp(0, length_mm);
    let to_edge_deg = (d2 as f64).sqrt() / FIXED_FACTOR;
    Some(Split {
        edge: (2 * k) as u32,
        vertex0: net.pair_from[k],
        vertex1: net.pair_to[k],
        distance0_mm: d0,
        distance1_mm: length_mm - d0,
        distance_to_edge_mm: java_int(to_edge_deg * METERS_PER_DEGREE_LAT * 1000.0),
        seg: best_seg as u32,
        fixed_lat: split_lat,
        fixed_lon: split_lon,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::osm::pbf::{Extract, Tags, Way};
    use crate::street::build::network_from_extract;
    use crate::street::parkride::get_or_create_vertex_near;
    use crate::street::Mode;

    /// Deterministic pseudo-random numbers for the tests (xorshift).
    pub(crate) struct Rng(pub u64);
    impl Rng {
        pub fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        /// Uniform in [0, 1).
        pub fn unit(&mut self) -> f64 {
            (self.next() >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// An `n` x `n` grid of residential streets from (`lat0`, `lon0`), `step`
    /// degrees between intersections, with two shape points on every block
    /// side; every position is jittered so edges cross cell boundaries at
    /// arbitrary places.
    pub(crate) fn jittered_grid(
        rng: &mut Rng,
        n: usize,
        lat0: f64,
        lon0: f64,
        step: f64,
    ) -> Extract {
        let fixed = |d: f64| (d * 1e7).round() as i32;
        let jitter = |rng: &mut Rng| (rng.unit() - 0.5) * step * 0.3;
        let mut pos = vec![(0.0, 0.0); n * n];
        for r in 0..n {
            for c in 0..n {
                pos[r * n + c] = (
                    lat0 + r as f64 * step + jitter(rng),
                    lon0 + c as f64 * step + jitter(rng),
                );
            }
        }
        let mut ex = Extract::default();
        for (i, &(lat, lon)) in pos.iter().enumerate() {
            ex.node_ids.push(1 + i as i64);
            ex.lat.push(fixed(lat));
            ex.lon.push(fixed(lon));
            ex.ref_counts.push(2);
        }
        let mut next_id = 1 + (n * n) as i64;
        let tags = || Tags(vec![("highway".into(), "residential".into())]);
        for vertical in [false, true] {
            for line in 0..n {
                let at = |i: usize| if vertical { i * n + line } else { line * n + i };
                let mut nodes = Vec::new();
                for i in 0..n {
                    nodes.push(1 + at(i) as i64);
                    if i + 1 == n {
                        break;
                    }
                    let (a, b) = (pos[at(i)], pos[at(i + 1)]);
                    for t in [1.0 / 3.0, 2.0 / 3.0] {
                        ex.node_ids.push(next_id);
                        ex.lat
                            .push(fixed(a.0 + t * (b.0 - a.0) + jitter(rng) * 0.3));
                        ex.lon
                            .push(fixed(a.1 + t * (b.1 - a.1) + jitter(rng) * 0.3));
                        ex.ref_counts.push(1);
                        nodes.push(next_id);
                        next_id += 1;
                    }
                }
                ex.ways.push(Way {
                    id: 1 + (usize::from(vertical) * n + line) as i64,
                    tags: tags(),
                    nodes,
                });
            }
        }
        ex
    }

    /// The index returns a subset of the pairs, so selection over it must
    /// give what selection over every pair gives, including after many edge
    /// splits (the split pairs are indexed by cell), for points inside and
    /// outside the network and both link radii.
    #[test]
    fn indexed_candidates_select_like_all_pairs() {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        let (n, lat0, lon0, step) = (20, -23.6, -46.7, 0.004);
        let mut net = network_from_extract(jittered_grid(&mut rng, n, lat0, lon0, step));
        let span = step * (n - 1) as f64;
        let point = |rng: &mut Rng| {
            (
                lat0 - 0.02 + rng.unit() * (span + 0.04),
                lon0 - 0.02 + rng.unit() * (span + 0.04),
            )
        };
        let mut scratch = Vec::new();
        let mut checked = 0;
        for i in 0..1500 {
            let (lat, lon) = point(&mut rng);
            get_or_create_vertex_near(&mut net, lat, lon, Mode::Walk);
            if i % 50 != 0 {
                continue;
            }
            let all: Vec<u32> = (0..net.pair_from.len() as u32).collect();
            for _ in 0..50 {
                let (lat, lon) = point(&mut rng);
                for mode in [Mode::Walk, Mode::Car] {
                    for radius in [INITIAL_LINK_RADIUS_M, LINK_RADIUS_M] {
                        let indexed = find_split_radius(&net, lat, lon, radius, mode, &mut scratch);
                        let every = select_split(&net, lat, lon, radius, mode, &all);
                        assert_eq!(indexed, every, "({lat}, {lon}) radius {radius}");
                        checked += usize::from(every.is_some());
                    }
                }
            }
        }
        assert!(checked > 3_000, "{checked}");
        assert!(net.pair_from.len() > 1_500, "{}", net.pair_from.len());
    }

    /// Two small networks 60 degrees apart: the index holds the occupied
    /// cells only, and queries in either area select as over every pair.
    #[test]
    fn far_apart_areas_index_only_occupied_cells() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let mut ex = jittered_grid(&mut rng, 8, -30.0, -51.0, 0.002);
        let mut far = jittered_grid(&mut rng, 8, 30.0, 9.0, 0.002);
        const SHIFT: i64 = 1_000_000;
        for id in &mut far.node_ids {
            *id += SHIFT;
        }
        for w in &mut far.ways {
            w.id += SHIFT;
            for n in &mut w.nodes {
                *n += SHIFT;
            }
        }
        ex.node_ids.append(&mut far.node_ids);
        ex.lat.append(&mut far.lat);
        ex.lon.append(&mut far.lon);
        ex.ref_counts.append(&mut far.ref_counts);
        ex.ways.append(&mut far.ways);
        let net = network_from_extract(ex);
        // A dense grid would have about 12,000 x 12,000 cells.
        assert!(net.index.cells.len() < 100, "{}", net.index.cells.len());
        let all: Vec<u32> = (0..net.pair_from.len() as u32).collect();
        let mut scratch = Vec::new();
        for (lat0, lon0) in [(-30.0, -51.0), (30.0, 9.0)] {
            for _ in 0..500 {
                let lat = lat0 - 0.01 + rng.unit() * 0.035;
                let lon = lon0 - 0.01 + rng.unit() * 0.035;
                for radius in [INITIAL_LINK_RADIUS_M, LINK_RADIUS_M] {
                    assert_eq!(
                        find_split_radius(&net, lat, lon, radius, Mode::Walk, &mut scratch),
                        select_split(&net, lat, lon, radius, Mode::Walk, &all)
                    );
                }
            }
        }
    }
}
