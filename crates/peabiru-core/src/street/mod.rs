//! Street network for walk, bike and car routing, built from OpenStreetMap
//! with the semantics of Conveyal R5 7.6, so travel times match r5r.
//!
//! The network stores edge *pairs*: pair `k` has the forward edge `2k`
//! (along the OSM way) and the backward edge `2k + 1`. Coordinates are
//! fixed-point degrees (1e-7), lengths millimetres, times whole seconds.

pub(crate) mod build;
mod ch;
pub(crate) mod geom;
pub(crate) mod labels;
pub(crate) mod link;
pub(crate) mod p2p;
pub(crate) mod parkride;
pub(crate) mod restrictions;
pub(crate) mod router;
pub(crate) mod scenario;

pub use build::{build_network, BuildOptions};
pub(crate) use restrictions::TurnRestriction;
pub use router::{street_travel_time_matrix, Mode, RouteParams, StreetEngine, StreetMatrix};
pub use scenario::{CarSpeeds, LtsChange, LtsLine, Scenario, SpeedPolygon};

/// Edge flags (bit set per directed edge).
#[allow(missing_docs)]
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
    /// Some turn restriction starts when a car enters this edge.
    pub const RESTRICTION_START: u32 = 1 << 12;
    /// A connector (park-and-ride, later transit stops): every mode may use
    /// it, points never link to it, and two in a row are not allowed.
    pub const LINK: u32 = 1 << 13;
    /// A `junction=roundabout` way (R5's `ROUNDABOUT`; only r5r's
    /// `osm_link_ids` reads it).
    pub const ROUNDABOUT: u32 = 1 << 14;

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
    /// The centre of a park-and-ride (R5 `VertexFlag.PARK_AND_RIDE`).
    pub const PARK_AND_RIDE: u8 = 1 << 2;
}

/// A routable street network for walking, cycling and driving, built by
/// [`build_network`] or, with its transit stops linked to the streets, by
/// [`build_from_dir`](crate::build_from_dir).
///
/// A network is read-only once built: share it between threads by
/// reference, as every routing function takes `&StreetNetwork`. Street
/// matrices may build contraction hierarchies and keep them in the network
/// for later calls (a few megabytes for a city; see [`StreetEngine`]), and
/// travel-time surfaces keep the links of their grid's cells to the
/// streets (40 bytes a cell, at most 64 MiB in all but for the last one
/// used; see [`street_travel_time_surface`](crate::street_travel_time_surface));
/// a clone shares them.
#[derive(Debug, Clone, Default)]
pub struct StreetNetwork {
    // Invariant: the arrays below never change once a network has been
    // handed out. They are written only while building, by
    // `build::network_from_extract` and, in `build_from_dir`, by
    // `TransitLayer::link_stops`, both before any street matrix can have
    // run, so the hierarchies kept in `hierarchies` and the grid linkages
    // kept in `grid_links` (built from these arrays) are never stale.
    // `link_stops` drops them all the same. Any new code that changes the
    // arrays of a network that may have been routed on must drop them too
    // (`forget_kept`).
    /// Vertex latitude, fixed-point 1e-7 degrees.
    pub(crate) v_lat: Vec<i32>,
    /// Vertex longitude, fixed-point 1e-7 degrees.
    pub(crate) v_lon: Vec<i32>,
    pub(crate) v_flags: Vec<u8>,

    /// Per pair: from and to vertex of the forward edge.
    pub(crate) pair_from: Vec<u32>,
    pub(crate) pair_to: Vec<u32>,
    /// Per pair: length in millimetres (haversine over the OSM nodes).
    pub(crate) pair_length_mm: Vec<i32>,
    /// Per pair: OSM way id.
    pub(crate) pair_way: Vec<i64>,
    /// Per pair: R5 street class code (`StreetClass.forWay`: 0 motorway,
    /// 1 primary, 2 secondary, 3 tertiary, 4 other), copied by splits.
    pub(crate) pair_class: Vec<u8>,
    /// R5 `StreetLayer.envelope`: the OSM nodes of every way segment an edge
    /// was attempted for, as `(min_lat, max_lat, min_lon, max_lon)` in node
    /// fixed-point degrees; `None` when no node was seen.
    pub(crate) envelope: Option<(i32, i32, i32, i32)>,
    /// Per pair: intermediate geometry `geom[a..b]` for `pair_geom[k] = (a, b)`,
    /// as `(lat, lon)` fixed-point, in forward order. A split appends the new
    /// geometry and moves the range.
    pub(crate) pair_geom: Vec<(u32, u32)>,
    pub(crate) geom: Vec<(i32, i32)>,
    /// Per pair: bearing leaving the first point and entering the last one,
    /// in binary radians (R5 brads).
    pub(crate) pair_in_brad: Vec<i8>,
    pub(crate) pair_out_brad: Vec<i8>,

    /// Per directed edge: flags and car speed in cm/s.
    pub(crate) edge_flags: Vec<u32>,
    pub(crate) edge_speed_cms: Vec<i16>,

    /// Outgoing edges of each vertex: `out_edges[out_offsets[v]..out_offsets[v + 1]]`.
    pub(crate) out_offsets: Vec<u32>,
    pub(crate) out_edges: Vec<u32>,
    /// Incoming edges of each vertex.
    pub(crate) in_offsets: Vec<u32>,
    pub(crate) in_edges: Vec<u32>,

    /// Spatial index over the forward edges, for linking.
    pub(crate) index: link::EdgeIndex,

    /// Car turn restrictions.
    pub(crate) restrictions: Vec<TurnRestriction>,
    /// The restrictions of reverse car searches: R5 turns each `only_`
    /// restriction into `no_` ones for them (`addReverseTurnRestriction`).
    pub(crate) reverse_restrictions: Vec<TurnRestriction>,
    /// `(from_edge, restriction index)`, sorted: restrictions that start when
    /// a car enters an edge.
    pub(crate) restriction_starts: Vec<(u32, u32)>,

    /// The slope cost function the network was built with, and per
    /// directed edge its slope factor (empty without elevation): see
    /// [`crate::Elevation`].
    pub(crate) elevation: crate::Elevation,
    pub(crate) edge_slope: Vec<f32>,
    /// What looked wrong in the elevation raster (see
    /// [`StreetNetwork::elevation_warnings`]).
    pub(crate) elevation_warnings: Vec<String>,

    /// Contraction hierarchies built by street matrices, kept for later
    /// ones (see [`StreetEngine`]); a clone shares them.
    hierarchies: ch::Cache,
    /// Links of the cells of travel-time surface grids, kept for later
    /// surfaces; a clone shares them.
    pub(crate) grid_links: crate::isochrone::linkage::Cache,
}

impl StreetNetwork {
    /// Number of vertices: street intersections and ends, split points,
    /// and the vertices of transit stops and park-and-ride connectors.
    pub fn n_vertices(&self) -> usize {
        self.v_lat.len()
    }

    /// Number of directed edges (two per street segment, one each way,
    /// whether or not a mode may use it).
    pub fn n_edges(&self) -> usize {
        self.edge_flags.len()
    }

    /// The slope cost function walking and cycling times use on this
    /// network: [`Elevation::None`](crate::Elevation::None) unless it was
    /// built with an elevation raster and a model (see
    /// [`BuildOptions::elevation`]).
    pub fn elevation(&self) -> crate::Elevation {
        self.elevation
    }

    /// Warnings about the elevation raster the network was built with
    /// (empty without one): samples whose value R5 stores wrapped around
    /// (beyond +/-3276.7 m, such as SRTM's -32768 voids, which become 0 m,
    /// and a no-data value of -9999, which becomes 3108.2 m) or equal to
    /// the raster's declared no-data value. peabiru stores them as r5r
    /// does, so the slope costs are r5r's; r5r says nothing about them.
    /// The core never prints them; the packages turn them into warnings.
    pub fn elevation_warnings(&self) -> &[String] {
        &self.elevation_warnings
    }

    /// Number of car turn restrictions read from OpenStreetMap and kept.
    pub fn n_restrictions(&self) -> usize {
        self.restrictions.len()
    }

    /// Drop the contraction hierarchies kept for street matrices and the
    /// grid linkages kept for surfaces: to be called by anything that
    /// changes the network's arrays (see the invariant on the fields).
    pub(crate) fn forget_kept(&mut self) {
        self.hierarchies = ch::Cache::default();
        self.grid_links = Default::default();
    }

    /// Indices of the restrictions that start on edge `e`.
    #[inline]
    pub(crate) fn restrictions_starting_at(&self, e: u32) -> impl Iterator<Item = u32> + '_ {
        let lo = self.restriction_starts.partition_point(|&(f, _)| f < e);
        self.restriction_starts[lo..]
            .iter()
            .take_while(move |&&(f, _)| f == e)
            .map(|&(_, r)| r)
    }

    #[inline]
    pub(crate) fn edge_from(&self, e: usize) -> usize {
        let k = e / 2;
        (if e % 2 == 0 {
            self.pair_from[k]
        } else {
            self.pair_to[k]
        }) as usize
    }

    #[inline]
    pub(crate) fn edge_to(&self, e: usize) -> usize {
        let k = e / 2;
        (if e % 2 == 0 {
            self.pair_to[k]
        } else {
            self.pair_from[k]
        }) as usize
    }

    #[inline]
    pub(crate) fn out_edges(&self, v: usize) -> &[u32] {
        &self.out_edges[self.out_offsets[v] as usize..self.out_offsets[v + 1] as usize]
    }

    #[inline]
    pub(crate) fn in_edges(&self, v: usize) -> &[u32] {
        &self.in_edges[self.in_offsets[v] as usize..self.in_offsets[v + 1] as usize]
    }

    /// All points of a pair's forward geometry, vertices included, as
    /// `(lat, lon)` fixed-point.
    pub(crate) fn pair_points(&self, k: usize) -> impl Iterator<Item = (i32, i32)> + '_ {
        let v0 = self.pair_from[k] as usize;
        let v1 = self.pair_to[k] as usize;
        let (a, b) = self.pair_geom[k];
        let mid = &self.geom[a as usize..b as usize];
        std::iter::once((self.v_lat[v0], self.v_lon[v0]))
            .chain(mid.iter().copied())
            .chain(std::iter::once((self.v_lat[v1], self.v_lon[v1])))
    }

    /// Bearing in whole degrees entering a directed edge (R5 `getInAngle`).
    #[inline]
    pub(crate) fn in_angle(&self, e: usize) -> i32 {
        let k = e / 2;
        if e % 2 == 0 {
            geom::brads_to_degrees(self.pair_in_brad[k])
        } else {
            geom::brads_to_degrees((self.pair_out_brad[k] as i32 + 128) as i8)
        }
    }

    /// Bearing in whole degrees leaving a directed edge (R5 `getOutAngle`).
    #[inline]
    pub(crate) fn out_angle(&self, e: usize) -> i32 {
        let k = e / 2;
        if e % 2 == 0 {
            geom::brads_to_degrees(self.pair_out_brad[k])
        } else {
            geom::brads_to_degrees((self.pair_in_brad[k] as i32 + 128) as i8)
        }
    }
}
