//! Linking points to the street network (R5 `Split.find`).

use super::flags::*;
use super::geom::{java_int, segment_fraction, FIXED_FACTOR};
use super::StreetNetwork;

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

/// Uniform grid over forward-edge bounding boxes, in fixed-point degrees.
#[derive(Debug, Clone, Default)]
pub struct EdgeIndex {
    min_lat: i32,
    min_lon: i32,
    n_rows: usize,
    n_cols: usize,
    /// Pairs in each cell: `pairs[offsets[c]..offsets[c + 1]]`.
    offsets: Vec<u32>,
    pairs: Vec<u32>,
    /// Pairs added after the grid was built (edge splits); every query
    /// returns them, which keeps queries a superset.
    extra: Vec<u32>,
}

/// Cell side in fixed-point degrees (0.005 degrees, about 550 m).
const CELL: i64 = 50_000;

impl EdgeIndex {
    pub fn build(net: &StreetNetwork) -> Self {
        let n_pairs = net.pair_from.len();
        if n_pairs == 0 {
            return EdgeIndex::default();
        }
        let bbox = |k: usize| {
            let mut b = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for (lat, lon) in net.pair_points(k) {
                b = (b.0.min(lat), b.1.min(lon), b.2.max(lat), b.3.max(lon));
            }
            b
        };
        let boxes: Vec<_> = (0..n_pairs).map(bbox).collect();
        let min_lat = boxes.iter().map(|b| b.0).min().unwrap();
        let min_lon = boxes.iter().map(|b| b.1).min().unwrap();
        let max_lat = boxes.iter().map(|b| b.2).max().unwrap();
        let max_lon = boxes.iter().map(|b| b.3).max().unwrap();
        let n_rows = ((i64::from(max_lat) - i64::from(min_lat)) / CELL + 1) as usize;
        let n_cols = ((i64::from(max_lon) - i64::from(min_lon)) / CELL + 1) as usize;
        let mut idx = EdgeIndex {
            min_lat,
            min_lon,
            n_rows,
            n_cols,
            offsets: vec![0; n_rows * n_cols + 1],
            pairs: Vec::new(),
            extra: Vec::new(),
        };
        let cells = |b: &(i32, i32, i32, i32)| {
            let r0 = ((i64::from(b.0) - i64::from(min_lat)) / CELL) as usize;
            let r1 = ((i64::from(b.2) - i64::from(min_lat)) / CELL) as usize;
            let c0 = ((i64::from(b.1) - i64::from(min_lon)) / CELL) as usize;
            let c1 = ((i64::from(b.3) - i64::from(min_lon)) / CELL) as usize;
            (r0..=r1).flat_map(move |r| (c0..=c1).map(move |c| r * n_cols + c))
        };
        for b in &boxes {
            for c in cells(b) {
                idx.offsets[c + 1] += 1;
            }
        }
        for c in 0..n_rows * n_cols {
            idx.offsets[c + 1] += idx.offsets[c];
        }
        idx.pairs = vec![0; idx.offsets[n_rows * n_cols] as usize];
        let mut cursor = idx.offsets.clone();
        for (k, b) in boxes.iter().enumerate() {
            for c in cells(b) {
                idx.pairs[cursor[c] as usize] = k as u32;
                cursor[c] += 1;
            }
        }
        idx
    }

    /// Register a pair created after the grid was built.
    pub fn insert_extra(&mut self, pair: u32) {
        self.extra.push(pair);
    }

    /// Pairs whose cells overlap an envelope (with repeats), fixed-point.
    pub fn query(
        &self,
        min_lat: i64,
        min_lon: i64,
        max_lat: i64,
        max_lon: i64,
        out: &mut Vec<u32>,
    ) {
        out.clear();
        out.extend_from_slice(&self.extra);
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
        for r in clamp_row(min_lat)..=clamp_row(max_lat) {
            for c in clamp_col(min_lon)..=clamp_col(max_lon) {
                let cell = r * self.n_cols + c;
                out.extend_from_slice(
                    &self.pairs[self.offsets[cell] as usize..self.offsets[cell + 1] as usize],
                );
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
    let sq_radius = radius_lat * radius_lat;
    let flag = link_flag(mode) | LINKABLE;

    net.index.query(
        i64::from(fixed_lat) - radius_lat,
        i64::from(fixed_lon) - radius_lon,
        i64::from(fixed_lat) + radius_lat,
        i64::from(fixed_lon) + radius_lon,
        scratch,
    );
    scratch.sort_unstable();
    scratch.dedup();

    // best: (squared distance, pair, segment, fraction, split lat, split lon)
    let mut best: Option<(i64, usize, usize, f64, i32, i32)> = None;
    for &k in scratch.iter() {
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
