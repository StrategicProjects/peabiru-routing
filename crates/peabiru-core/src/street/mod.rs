//! Street network for walk, bike and car routing, built from OpenStreetMap
//! with the semantics of Conveyal R5 7.6, so travel times match r5r.
//!
//! The network stores edge *pairs*: pair `k` has the forward edge `2k`
//! (along the OSM way) and the backward edge `2k + 1`. Coordinates are
//! fixed-point degrees (1e-7), lengths millimetres, times whole seconds.

pub mod build;
pub mod geom;
pub mod labels;
pub mod link;
pub mod router;

pub use build::{build_network, BuildOptions};
pub use link::Split;
pub use router::{travel_time_matrix, Mode, RouteParams};

/// Edge flags (bit set per directed edge).
pub mod flags {
    pub const ALLOWS_PEDESTRIAN: u32 = 1 << 0;
    pub const ALLOWS_BIKE: u32 = 1 << 1;
    pub const ALLOWS_CAR: u32 = 1 << 2;
    pub const NO_THRU_TRAFFIC_PEDESTRIAN: u32 = 1 << 3;
    pub const NO_THRU_TRAFFIC_BIKE: u32 = 1 << 4;
    pub const NO_THRU_TRAFFIC_CAR: u32 = 1 << 5;
    pub const LINKABLE: u32 = 1 << 6;
    pub const BIKE_LTS_1: u32 = 1 << 7;
    pub const BIKE_LTS_2: u32 = 1 << 8;
    pub const BIKE_LTS_3: u32 = 1 << 9;
    pub const BIKE_LTS_4: u32 = 1 << 10;
    pub const BIKE_LTS_EXPLICIT: u32 = 1 << 11;

    pub const ALL_ALLOWS: u32 = ALLOWS_PEDESTRIAN | ALLOWS_BIKE | ALLOWS_CAR;
    /// Flags that make an edge worth creating (R5 `ALL_PERMISSIONS`).
    pub const ALL_PERMISSIONS: u32 =
        ALL_ALLOWS | NO_THRU_TRAFFIC_PEDESTRIAN | NO_THRU_TRAFFIC_BIKE | NO_THRU_TRAFFIC_CAR;
    pub const LTS_MASK: u32 = BIKE_LTS_1 | BIKE_LTS_2 | BIKE_LTS_3 | BIKE_LTS_4;
}

/// Vertex flags.
pub mod vertex_flags {
    pub const TRAFFIC_SIGNAL: u8 = 1 << 0;
    pub const IMPASSABLE: u8 = 1 << 1;
}

/// A routable street network.
#[derive(Debug, Clone, Default)]
pub struct StreetNetwork {
    /// Vertex latitude, fixed-point 1e-7 degrees.
    pub v_lat: Vec<i32>,
    /// Vertex longitude, fixed-point 1e-7 degrees.
    pub v_lon: Vec<i32>,
    pub v_flags: Vec<u8>,

    /// Per pair: from and to vertex of the forward edge.
    pub pair_from: Vec<u32>,
    pub pair_to: Vec<u32>,
    /// Per pair: length in millimetres (haversine over the OSM nodes).
    pub pair_length_mm: Vec<i32>,
    /// Per pair: OSM way id.
    pub pair_way: Vec<i64>,
    /// Per pair: intermediate geometry `geom[geom_offsets[k]..geom_offsets[k + 1]]`
    /// as `(lat, lon)` fixed-point, in forward order.
    pub geom_offsets: Vec<u32>,
    pub geom: Vec<(i32, i32)>,
    /// Per pair: bearing leaving the first point and entering the last one,
    /// in binary radians (R5 brads).
    pub pair_in_brad: Vec<i8>,
    pub pair_out_brad: Vec<i8>,

    /// Per directed edge: flags and car speed in cm/s.
    pub edge_flags: Vec<u32>,
    pub edge_speed_cms: Vec<i16>,

    /// Outgoing edges of each vertex: `out_edges[out_offsets[v]..out_offsets[v + 1]]`.
    pub out_offsets: Vec<u32>,
    pub out_edges: Vec<u32>,
    /// Incoming edges of each vertex.
    pub in_offsets: Vec<u32>,
    pub in_edges: Vec<u32>,

    /// Spatial index over the forward edges, for linking.
    pub index: link::EdgeIndex,
}

impl StreetNetwork {
    pub fn n_vertices(&self) -> usize {
        self.v_lat.len()
    }

    pub fn n_edges(&self) -> usize {
        self.edge_flags.len()
    }

    #[inline]
    pub fn edge_from(&self, e: usize) -> usize {
        let k = e / 2;
        (if e % 2 == 0 {
            self.pair_from[k]
        } else {
            self.pair_to[k]
        }) as usize
    }

    #[inline]
    pub fn edge_to(&self, e: usize) -> usize {
        let k = e / 2;
        (if e % 2 == 0 {
            self.pair_to[k]
        } else {
            self.pair_from[k]
        }) as usize
    }

    #[inline]
    pub fn out_edges(&self, v: usize) -> &[u32] {
        &self.out_edges[self.out_offsets[v] as usize..self.out_offsets[v + 1] as usize]
    }

    #[inline]
    pub fn in_edges(&self, v: usize) -> &[u32] {
        &self.in_edges[self.in_offsets[v] as usize..self.in_offsets[v + 1] as usize]
    }

    /// All points of a pair's forward geometry, vertices included, as
    /// `(lat, lon)` fixed-point.
    pub fn pair_points(&self, k: usize) -> impl Iterator<Item = (i32, i32)> + '_ {
        let v0 = self.pair_from[k] as usize;
        let v1 = self.pair_to[k] as usize;
        let mid = &self.geom[self.geom_offsets[k] as usize..self.geom_offsets[k + 1] as usize];
        std::iter::once((self.v_lat[v0], self.v_lon[v0]))
            .chain(mid.iter().copied())
            .chain(std::iter::once((self.v_lat[v1], self.v_lon[v1])))
    }

    /// Bearing in whole degrees entering a directed edge (R5 `getInAngle`).
    #[inline]
    pub fn in_angle(&self, e: usize) -> i32 {
        let k = e / 2;
        if e % 2 == 0 {
            geom::brads_to_degrees(self.pair_in_brad[k])
        } else {
            geom::brads_to_degrees((self.pair_out_brad[k] as i32 + 128) as i8)
        }
    }

    /// Bearing in whole degrees leaving a directed edge (R5 `getOutAngle`).
    #[inline]
    pub fn out_angle(&self, e: usize) -> i32 {
        let k = e / 2;
        if e % 2 == 0 {
            geom::brads_to_degrees(self.pair_out_brad[k])
        } else {
            geom::brads_to_degrees((self.pair_in_brad[k] as i32 + 128) as i8)
        }
    }
}
