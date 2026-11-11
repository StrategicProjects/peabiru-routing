//! The street edges of line-based isochrones: r5r's
//! `street_network_to_sf()$edges` (r5r's Java `StreetNetwork`) and the
//! centroid of each edge that r5r routes to (`sf::st_centroid()`, which is
//! `s2::s2_centroid()` for longitude/latitude data).

use crate::street::flags::*;
use crate::street::geom::FIXED_FACTOR;
use crate::street::StreetNetwork;

/// The street edges of a network as r5r's `street_network_to_sf()` lists
/// them: every directed edge but the connectors of transit stops and
/// park-and-rides, in edge order, with their attributes, geometry and
/// centroid. Made by [`street_edges`].
#[derive(Debug, Clone)]
pub struct StreetEdges {
    /// Directed edge index of each row.
    edges: Vec<u32>,
    /// Per row: `(lat, lon)` of the centroid, `None` for a zero-length
    /// geometry (s2 gives an empty point).
    centroids: Vec<Option<(f64, f64)>>,
    osm_id: Vec<i64>,
    from_vertex: Vec<u32>,
    to_vertex: Vec<u32>,
    class: Vec<u8>,
    length_mm: Vec<i32>,
    flags: Vec<u32>,
    speed_cms: Vec<i16>,
    /// Per row: `geometry[geom_offsets[i]..geom_offsets[i + 1]]`, `(lon, lat)`.
    geom_offsets: Vec<u32>,
    geometry: Vec<[f64; 2]>,
}

/// The street edges of `net`, as r5r's `street_network_to_sf(r5r_network)$edges`
/// (see [`StreetEdges`]).
pub fn street_edges(net: &StreetNetwork) -> StreetEdges {
    let mut t = StreetEdges {
        edges: Vec::new(),
        centroids: Vec::new(),
        osm_id: Vec::new(),
        from_vertex: Vec::new(),
        to_vertex: Vec::new(),
        class: Vec::new(),
        length_mm: Vec::new(),
        flags: Vec::new(),
        speed_cms: Vec::new(),
        geom_offsets: vec![0],
        geometry: Vec::new(),
    };
    let deg = |f: i32| f64::from(f) / FIXED_FACTOR;
    let mut pts: Vec<[f64; 2]> = Vec::new();
    for e in 0..net.n_edges() {
        let f = net.edge_flags[e];
        if f & LINK != 0 {
            continue;
        }
        let k = e / 2;
        // EdgeStore.Edge.getGeometry(): the pair's points, reversed for a
        // backward edge
        pts.clear();
        pts.extend(net.pair_points(k).map(|(lat, lon)| [deg(lon), deg(lat)]));
        if e % 2 == 1 {
            pts.reverse();
        }
        t.edges.push(e as u32);
        t.centroids.push(s2_centroid(&pts));
        t.osm_id.push(net.pair_way[k]);
        t.from_vertex.push(net.edge_from(e) as u32);
        t.to_vertex.push(net.edge_to(e) as u32);
        t.class.push(net.pair_class.get(k).copied().unwrap_or(4));
        t.length_mm.push(net.pair_length_mm[k]);
        t.flags.push(f);
        t.speed_cms.push(net.edge_speed_cms[e]);
        t.geometry.extend_from_slice(&pts);
        t.geom_offsets.push(t.geometry.len() as u32);
    }
    t
}

impl StreetEdges {
    /// Number of edges (rows).
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    /// Whether there are no edges.
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// `edge_index`: the directed edge index in the network (R5's).
    pub fn edge_index(&self, i: usize) -> u32 {
        self.edges[i]
    }

    /// `osm_id`: the OpenStreetMap way the edge was made from.
    pub fn osm_id(&self, i: usize) -> i64 {
        self.osm_id[i]
    }

    /// `from_vertex`.
    pub fn from_vertex(&self, i: usize) -> u32 {
        self.from_vertex[i]
    }

    /// `to_vertex`.
    pub fn to_vertex(&self, i: usize) -> u32 {
        self.to_vertex[i]
    }

    /// `street_class`: `"MOTORWAY"`, `"PRIMARY"`, `"SECONDARY"`,
    /// `"TERTIARY"` or `"OTHER"` (from the way's `highway` tag).
    pub fn street_class(&self, i: usize) -> &'static str {
        match self.class[i] {
            0 => "MOTORWAY",
            1 => "PRIMARY",
            2 => "SECONDARY",
            3 => "TERTIARY",
            _ => "OTHER",
        }
    }

    /// `length` in metres.
    pub fn length_m(&self, i: usize) -> f64 {
        f64::from(self.length_mm[i]) / 1000.0
    }

    /// `walk`: whether pedestrians may use the edge.
    pub fn walk(&self, i: usize) -> bool {
        self.flags[i] & ALLOWS_PEDESTRIAN != 0
    }

    /// `car`: whether cars may use the edge.
    pub fn car(&self, i: usize) -> bool {
        self.flags[i] & ALLOWS_CAR != 0
    }

    /// `car_speed` in km/h, as R5 reports it (`getSpeedKph`, a `float`).
    pub fn car_speed_kph(&self, i: usize) -> f64 {
        f64::from(((f64::from(self.speed_cms[i]) / 100.0) * 3.6) as f32)
    }

    /// `bicycle`: whether bicycles may use the edge.
    pub fn bicycle(&self, i: usize) -> bool {
        self.flags[i] & ALLOWS_BIKE != 0
    }

    /// `bicycle_lts`: level of traffic stress, 1 to 4 (1 when none is set).
    pub fn bicycle_lts(&self, i: usize) -> i32 {
        let f = self.flags[i];
        let mut lts = 1;
        if f & BIKE_LTS_2 != 0 {
            lts = 2;
        }
        if f & BIKE_LTS_3 != 0 {
            lts = 3;
        }
        if f & BIKE_LTS_4 != 0 {
            lts = 4;
        }
        lts
    }

    /// The edge's line, `[lon, lat]` degrees from its start to its end.
    pub fn geometry(&self, i: usize) -> &[[f64; 2]] {
        &self.geometry[self.geom_offsets[i] as usize..self.geom_offsets[i + 1] as usize]
    }

    /// `(lat, lon)` degrees of the point r5r routes to for the edge: the
    /// centroid of its line on the sphere (`sf::st_centroid()` with s2),
    /// `None` when the line has no length.
    pub fn centroid(&self, i: usize) -> Option<(f64, f64)> {
        self.centroids[i]
    }

    /// Every centroid as `(lat, lon)`, NaN for an edge without one.
    pub fn centroid_points(&self) -> Vec<(f64, f64)> {
        self.centroids
            .iter()
            .map(|c| c.unwrap_or((f64::NAN, f64::NAN)))
            .collect()
    }
}

type Vec3 = [f64; 3];

/// s2 `S2LatLng::ToPoint` of a longitude/latitude, as s2's plate carrée
/// projection unprojects it.
fn to_point([lon, lat]: [f64; 2]) -> Vec3 {
    let to_radians = std::f64::consts::PI / 180.0;
    let phi = to_radians * lat;
    let theta = to_radians * libm_remainder(lon, 360.0);
    let cosphi = phi.cos();
    [theta.cos() * cosphi, theta.sin() * cosphi, phi.sin()]
}

/// C `remainder()` for the longitudes s2 sees: the identity within
/// `[-180, 180]`.
fn libm_remainder(x: f64, y: f64) -> f64 {
    let n = (x / y).round_ties_even();
    if n == 0.0 {
        x
    } else {
        x - n * y
    }
}

/// s2's `Vector3::Norm2`: `0 + x*x + y*y + z*z`, one term at a time.
fn norm2(v: Vec3) -> f64 {
    let mut s = 0.0;
    for c in v {
        s += c * c;
    }
    s
}

/// `S2::TrueCentroid(a, b)` of a segment: towards its midpoint, scaled by
/// twice the sine of half its angle.
fn true_centroid(a: Vec3, b: Vec3) -> Vec3 {
    let vdiff = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let vsum = [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
    let sin2 = norm2(vdiff);
    let cos2 = norm2(vsum);
    if cos2 == 0.0 {
        return [0.0; 3];
    }
    let s = (sin2 / cos2).sqrt();
    [s * vsum[0], s * vsum[1], s * vsum[2]]
}

/// `s2_centroid()` of a line (s2geography: the sum of the segments' true
/// centroids, normalised), as `(lat, lon)` degrees; `None` for a zero
/// vector.
fn s2_centroid(line: &[[f64; 2]]) -> Option<(f64, f64)> {
    let mut c = [0.0f64; 3];
    let mut prev = None;
    for &p in line {
        let q = to_point(p);
        if let Some(a) = prev {
            let t = true_centroid(a, q);
            for k in 0..3 {
                c[k] += t[k];
            }
        }
        prev = Some(q);
    }
    let n = norm2(c).sqrt();
    if n == 0.0 {
        return None;
    }
    let inv = 1.0 / n;
    let c = [c[0] * inv, c[1] * inv, c[2] * inv];
    // S2LatLng(point) and S1Angle::degrees()
    let to_degrees = 180.0 / std::f64::consts::PI;
    let lat = (c[2] + 0.0).atan2((c[0] * c[0] + c[1] * c[1]).sqrt());
    let lon = (c[1] + 0.0).atan2(c[0] + 0.0);
    Some((to_degrees * lat, to_degrees * lon))
}

/// One edge of a line-based isochrone: from origin `origin`, the edge in
/// row `edge` of the [`StreetEdges`] is reached in `travel_time` minutes,
/// within the band of cutoff `isochrone` (the smallest cutoff at or above
/// the travel time).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct IsochroneLine {
    /// Index of the origin.
    pub origin: usize,
    /// Row in the [`StreetEdges`] routed to.
    pub edge: usize,
    /// `travel_time_p50`, whole minutes, above 0.
    pub travel_time: u32,
    /// `isochrone`: the cutoff of the edge's band, minutes.
    pub isochrone: f64,
}

/// Line-based isochrones from a travel-time matrix to the edges'
/// centroids, as r5r's `isochrone(polygon_output = FALSE)` makes them from
/// `travel_time_matrix()`: `minutes` holds one row per origin and one value
/// per row of the [`StreetEdges`] (the median travel time, `None` where
/// unreached or beyond the trip limit), and `cutoffs` are r5r's (see
/// [`isochrone_cutoffs`](crate::isochrone_cutoffs)).
///
/// Times of 0 are left out; each other edge gets the band `(a, b]` of
/// consecutive cutoffs (starting from 0) holding its time, and edges beyond
/// the last cutoff are left out. The result is ordered by origin, then by
/// band and travel time, both descending, ties in edge order, as r5r
/// orders it. [`RouteError::Invalid`](crate::RouteError::Invalid) for bad
/// cutoffs or a matrix of the wrong size.
pub fn isochrone_lines(
    n_origins: usize,
    n_edges: usize,
    minutes: &[Option<u32>],
    cutoffs: &[f64],
) -> Result<Vec<IsochroneLine>, crate::RouteError> {
    let cutoffs = super::isochrone_cutoffs(cutoffs)?;
    if minutes.len() != n_origins * n_edges {
        return Err(crate::RouteError::Invalid(format!(
            "a matrix of {n_origins} x {n_edges} needs {} values, got {}",
            n_origins * n_edges,
            minutes.len()
        )));
    }
    let mut out = Vec::new();
    for o in 0..n_origins {
        let start = out.len();
        let row = &minutes[o * n_edges..(o + 1) * n_edges];
        for (edge, t) in row.iter().enumerate() {
            let Some(t) = *t else { continue };
            if t == 0 {
                continue;
            }
            // cut(x, breaks = c(0, cutoffs), labels = FALSE): right-closed
            let x = f64::from(t);
            let Some(&band) = cutoffs.iter().find(|&&c| x <= c) else {
                continue;
            };
            out.push(IsochroneLine {
                origin: o,
                edge,
                travel_time: t,
                isochrone: band,
            });
        }
        // order(-isochrone, -travel_time_p50), stable
        out[start..].sort_by(|a, b| {
            b.isochrone
                .total_cmp(&a.isochrone)
                .then(b.travel_time.cmp(&a.travel_time))
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centroid_of_a_straight_segment_is_its_midpoint() {
        let (lat, lon) = s2_centroid(&[[-51.0, -30.0], [-51.0, -29.0]]).unwrap();
        assert!((lat + 29.5).abs() < 1e-12, "{lat}");
        assert!((lon + 51.0).abs() < 1e-12, "{lon}");
        assert_eq!(s2_centroid(&[[-51.0, -30.0], [-51.0, -30.0]]), None);
        assert_eq!(libm_remainder(-51.5, 360.0), -51.5);
        assert_eq!(libm_remainder(190.0, 360.0), -170.0);
    }

    #[test]
    fn lines_are_banded_and_ordered_as_r5r() {
        let m = [
            Some(3),
            Some(0),
            None,
            Some(16),
            Some(15),
            Some(30),
            Some(31),
            Some(3),
        ];
        let lines = isochrone_lines(1, 8, &m, &[0.0, 15.0, 30.0]).unwrap();
        let got: Vec<(usize, u32, f64)> = lines
            .iter()
            .map(|l| (l.edge, l.travel_time, l.isochrone))
            .collect();
        assert_eq!(
            got,
            vec![
                (5, 30, 30.0),
                (3, 16, 30.0),
                (4, 15, 15.0),
                (0, 3, 15.0),
                (7, 3, 15.0)
            ]
        );
        assert!(isochrone_lines(1, 7, &m, &[15.0]).is_err());
    }
}
