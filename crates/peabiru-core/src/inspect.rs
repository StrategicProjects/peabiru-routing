//! What a network holds, as r5r's network functions report it:
//! `find_snap()`, `street_network_bbox()`, `street_network_to_sf()`,
//! `transit_network_to_sf()` and `check_transit_availability()` (r5r's
//! Java `SnapFinder`, `StreetNetwork`, `TransitNetwork` and
//! `TransitServiceDates`).

use crate::street::flags::LINK;
use crate::street::geom::FIXED_FACTOR;
use crate::street::link::find_split_radius;
use crate::street::vertex_flags::PARK_AND_RIDE;
use crate::street::{Mode, StreetNetwork};
use crate::transit::matrix::{check_date, weekday};
use crate::{RouteError, TransitLayer};

/// Where a point meets the street network: the result of [`find_snap`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Snap {
    /// Latitude of the snapped point, degrees.
    pub lat: f64,
    /// Longitude of the snapped point, degrees.
    pub lon: f64,
    /// Distance from the point to the snapped point, metres (haversine on
    /// R5's sphere of radius 6,367,450 m).
    pub distance_m: f64,
}

/// Where each point (`(lat, lon)` degrees) snaps to the streets `mode` may
/// use, as r5r's `find_snap()`: the nearest point of an edge within
/// `radius_m` metres, looked for first within 300 m when `radius_m` is
/// larger (R5 `StreetLayer.findSplit`). `None` for a point with no such
/// edge. [`RouteError::Invalid`] for a negative or non-finite radius.
pub fn find_snap(
    net: &StreetNetwork,
    points: &[(f64, f64)],
    radius_m: f64,
    mode: Mode,
) -> Result<Vec<Option<Snap>>, RouteError> {
    if !(radius_m.is_finite() && radius_m >= 0.0) {
        return Err(RouteError::Invalid(format!(
            "the radius must be a finite number of metres >= 0, got {radius_m}"
        )));
    }
    const INITIAL_RADIUS_M: f64 = crate::street::link::INITIAL_LINK_RADIUS_M;
    let mut scratch = Vec::new();
    Ok(points
        .iter()
        .map(|&(lat, lon)| {
            let first = if radius_m > INITIAL_RADIUS_M {
                find_split_radius(net, lat, lon, INITIAL_RADIUS_M, mode, &mut scratch)
            } else {
                None
            };
            let split =
                first.or_else(|| find_split_radius(net, lat, lon, radius_m, mode, &mut scratch))?;
            let snap_lat = f64::from(split.fixed_lat) / FIXED_FACTOR;
            let snap_lon = f64::from(split.fixed_lon) / FIXED_FACTOR;
            Some(Snap {
                lat: snap_lat,
                lon: snap_lon,
                distance_m: crate::fastmath_trig::haversine_m(lat, lon, snap_lat, snap_lon),
            })
        })
        .collect())
}

impl StreetNetwork {
    /// The bounding box of the street network as
    /// `[xmin, ymin, xmax, ymax]` (longitude, latitude degrees): R5's
    /// `StreetLayer.envelope`, which r5r's `street_network_bbox()` returns.
    /// It covers every OpenStreetMap node of the ways R5 tried to make
    /// street edges from, even edges it then dropped, so it can be a little
    /// larger than the box of the edges. A network without streets gives
    /// JTS's empty envelope, `[0, 0, -1, -1]`.
    pub fn bbox(&self) -> [f64; 4] {
        match self.envelope {
            Some((min_lat, max_lat, min_lon, max_lon)) => {
                let d = |f: i32| f64::from(f) / FIXED_FACTOR;
                [d(min_lon), d(min_lat), d(max_lon), d(max_lat)]
            }
            None => [0.0, 0.0, -1.0, -1.0],
        }
    }
}

/// The vertices of r5r's `street_network_to_sf()$vertices`: those at an end
/// of a street edge (connectors of transit stops and park-and-rides left
/// out), in r5r's order. Made by [`street_vertices`].
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct StreetVertices {
    /// `index`: the vertex index in the network.
    pub index: Vec<u32>,
    /// Latitude, degrees.
    pub lat: Vec<f64>,
    /// Longitude, degrees.
    pub lon: Vec<f64>,
    /// `park_and_ride`: the centre of a park-and-ride.
    pub park_and_ride: Vec<bool>,
    /// `bike_sharing`: a bike-sharing station (always `false`: neither r5r
    /// nor peabiru loads bike sharing).
    pub bike_sharing: Vec<bool>,
}

/// The street vertices r5r's `street_network_to_sf()` lists (see
/// [`StreetVertices`]). r5r collects them in a Java `HashSet<Integer>`
/// while it walks the edges, and lists them in that set's iteration order,
/// which this reproduces: by bucket of the set's final table (the hash of
/// an `Integer` spread as `h ^ (h >>> 16)`), and within a bucket in the
/// order they were added.
pub fn street_vertices(net: &StreetNetwork) -> StreetVertices {
    let mut seen = vec![false; net.n_vertices()];
    let mut order: Vec<u32> = Vec::new();
    for e in 0..net.n_edges() {
        if net.edge_flags[e] & LINK != 0 {
            continue;
        }
        for v in [net.edge_from(e), net.edge_to(e)] {
            if !seen[v] {
                seen[v] = true;
                order.push(v as u32);
            }
        }
    }
    let order = java_hash_set_order(order);
    let deg = |f: i32| f64::from(f) / FIXED_FACTOR;
    StreetVertices {
        lat: order.iter().map(|&v| deg(net.v_lat[v as usize])).collect(),
        lon: order.iter().map(|&v| deg(net.v_lon[v as usize])).collect(),
        park_and_ride: order
            .iter()
            .map(|&v| net.v_flags[v as usize] & PARK_AND_RIDE != 0)
            .collect(),
        bike_sharing: vec![false; order.len()],
        index: order,
    }
}

/// Iteration order of a `java.util.HashSet<Integer>` (default capacity and
/// load factor) after adding `keys`, distinct, in this order. A bucket is
/// `(h ^ (h >>> 16)) & (capacity - 1)`; resizing keeps the order within a
/// bucket, so the final order is by bucket, then by insertion. (Java turns
/// a bucket of more than 8 keys into a tree, which orders it differently;
/// that needs 9 keys whose spread hashes agree in their low bits, which
/// vertex indices, dense from 0, do not give.)
fn java_hash_set_order(keys: Vec<u32>) -> Vec<u32> {
    let mut capacity = 16usize;
    while keys.len() > capacity / 4 * 3 {
        capacity *= 2;
    }
    let mask = (capacity - 1) as u32;
    let bucket = |k: u32| (k ^ (k >> 16)) & mask;
    let mut keys = keys;
    keys.sort_by_key(|&k| bucket(k)); // stable
    keys
}

/// The stops of r5r's `transit_network_to_sf()$stops`, one per stop in
/// stop index order. Made by [`transit_stops`].
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct TransitStops {
    /// `stop_index`.
    pub stop_index: Vec<u32>,
    /// `stop_id`, prefixed by the feed id (`feed:stop`).
    pub stop_id: Vec<String>,
    /// `stop_name`; `None` when blank.
    pub stop_name: Vec<Option<String>>,
    /// Latitude of the stop's street vertex, degrees; `None` for a stop not
    /// linked to the streets.
    pub lat: Vec<Option<f64>>,
    /// Longitude, as `lat`.
    pub lon: Vec<Option<f64>>,
    /// `linked_to_street`.
    pub linked_to_street: Vec<bool>,
}

/// The transit stops of a network (see [`TransitStops`]): the position is
/// that of the stop's vertex in the street network, the stop's coordinates
/// rounded down to 1e-7 degrees, as r5r reports it.
pub fn transit_stops(net: &StreetNetwork, layer: &TransitLayer) -> TransitStops {
    let n = layer.n_stops();
    let mut t = TransitStops::default();
    for s in 0..n {
        let v = layer.stop_vertex[s];
        let at = (v >= 0).then_some(v as usize);
        t.stop_index.push(s as u32);
        t.stop_id.push(layer.stop_ids[s].clone());
        t.stop_name.push(layer.stop_names.get(s).cloned().flatten());
        t.lat
            .push(at.map(|v| f64::from(net.v_lat[v]) / FIXED_FACTOR));
        t.lon
            .push(at.map(|v| f64::from(net.v_lon[v]) / FIXED_FACTOR));
        t.linked_to_street.push(at.is_some());
    }
    t
}

/// The routes of r5r's `transit_network_to_sf()$routes`: one row per trip
/// pattern (the trips of a route with the same stops), in pattern order.
/// Made by [`transit_routes`].
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct TransitRoutes {
    /// `agency_id` of the route's agency.
    pub agency_id: Vec<Option<String>>,
    /// `agency_name`.
    pub agency_name: Vec<Option<String>>,
    /// `route_id`, as in the feed (not prefixed).
    pub route_id: Vec<String>,
    /// `route_long_name`.
    pub long_name: Vec<Option<String>>,
    /// `route_short_name`.
    pub short_name: Vec<Option<String>>,
    /// `mode`: R5's transit mode of the GTFS `route_type` (`"BUS"`,
    /// `"RAIL"`, `"SUBWAY"`, `"TRAM"`, `"FERRY"`, `"CABLE_CAR"`,
    /// `"GONDOLA"`, `"FUNICULAR"` or `"AIR"`).
    pub mode: Vec<&'static str>,
    /// Per row: `geometry[offsets[i]..offsets[i + 1]]`, `[lon, lat]`.
    offsets: Vec<u32>,
    geometry: Vec<[f64; 2]>,
}

impl TransitRoutes {
    /// Number of rows.
    pub fn len(&self) -> usize {
        self.route_id.len()
    }

    /// Whether there are no rows.
    pub fn is_empty(&self) -> bool {
        self.route_id.is_empty()
    }

    /// The line of row `i` as `[lon, lat]` degrees: the GTFS shape of the
    /// trip that made the pattern, or else the positions of its linked
    /// stops (empty when none is linked).
    pub fn geometry(&self, i: usize) -> &[[f64; 2]] {
        &self.geometry[self.offsets[i] as usize..self.offsets[i + 1] as usize]
    }
}

/// R5's `TransitLayer.getTransitModes`, by name; `None` for a route type R5
/// does not know (it fails on it).
fn transit_mode(route_type: i32) -> Option<&'static str> {
    Some(match route_type {
        100..=199 | 300..=499 => "RAIL",
        200..=299 | 700..=899 => "BUS",
        500..=699 => "SUBWAY",
        900..=999 => "TRAM",
        1000..=1099 | 1200..=1299 => "FERRY",
        1100..=1199 => "AIR",
        1300..=1399 => "GONDOLA",
        1400..=1499 => "FUNICULAR",
        0 | 12 => "TRAM",
        1 => "SUBWAY",
        2 => "RAIL",
        3 | 11 => "BUS",
        4 => "FERRY",
        5 => "CABLE_CAR",
        6 => "GONDOLA",
        7 => "FUNICULAR",
        _ => return None,
    })
}

/// The transit routes of a network (see [`TransitRoutes`]).
/// [`RouteError::Invalid`] for a route type R5 does not know (r5r fails
/// with Java's `IllegalArgumentException` there).
pub fn transit_routes(
    net: &StreetNetwork,
    layer: &TransitLayer,
) -> Result<TransitRoutes, RouteError> {
    let mut t = TransitRoutes {
        offsets: vec![0],
        ..TransitRoutes::default()
    };
    for (p, pattern) in layer.patterns.iter().enumerate() {
        let route = layer
            .pattern_route
            .get(p)
            .and_then(|&r| layer.routes.get(r as usize));
        let route_type = route.map_or(pattern.route_type, |r| r.route_type);
        let mode = transit_mode(route_type).ok_or_else(|| {
            RouteError::Invalid(format!("Unknown GTFS route_type code: {route_type}"))
        })?;
        t.agency_id.push(route.and_then(|r| r.agency_id.clone()));
        t.agency_name
            .push(route.and_then(|r| r.agency_name.clone()));
        t.route_id
            .push(route.map_or_else(|| pattern.route_id.clone(), |r| r.route_id.clone()));
        t.long_name.push(route.and_then(|r| r.long_name.clone()));
        t.short_name.push(route.and_then(|r| r.short_name.clone()));
        t.mode.push(mode);
        match layer.pattern_shape.get(p).copied().flatten() {
            Some(s) => t.geometry.extend_from_slice(&layer.shapes[s as usize]),
            None => {
                for &s in &pattern.stops {
                    let v = layer.stop_vertex[s as usize];
                    if v >= 0 {
                        let v = v as usize;
                        t.geometry.push([
                            f64::from(net.v_lon[v]) / FIXED_FACTOR,
                            f64::from(net.v_lat[v]) / FIXED_FACTOR,
                        ]);
                    }
                }
            }
        }
        t.offsets.push(t.geometry.len() as u32);
    }
    Ok(t)
}

/// The dates (`yyyymmdd`, ascending) on which some service of a trip in
/// the timetable runs, as r5r's `check_transit_availability()` finds them
/// when no date is given (r5r's `TransitServiceDates.getDates`): every day
/// of each such service's calendar on which it runs, and every date of its
/// calendar exceptions on which it runs. Services no trip uses do not
/// count. [`RouteError::Invalid`] when such a service's calendar has a
/// start or end that is not a date, with r5r's message.
pub fn transit_service_dates(layer: &TransitLayer) -> Result<Vec<u32>, RouteError> {
    let mut used = vec![false; layer.services.len()];
    for p in &layer.patterns {
        for t in &p.trips {
            if let Some(u) = used.get_mut(t.service as usize) {
                *u = true;
            }
        }
    }
    let mut dates = std::collections::BTreeSet::new();
    for (code, s) in layer.services.iter().enumerate() {
        if !used[code] {
            continue;
        }
        if let Some((_, start_int, end_int)) = s.calendar {
            let (Some(start), Some(end)) = (basic_iso_date(start_int), basic_iso_date(end_int))
            else {
                let (start, end) = (start_int, end_int);
                let id = layer.service_ids.get(code).map_or("", String::as_str);
                return Err(RouteError::Invalid(format!(
                    "Cannot discover operating dates for service '{id}': invalid calendar \
                     bounds {start} to {end}. Specify dates explicitly or correct the GTFS \
                     calendar."
                )));
            };
            let mut d = start;
            while d <= end {
                if s.active_on(d, weekday(d)) {
                    dates.insert(d);
                }
                d = next_day(d);
            }
        }
        for &d in s.exceptions.keys() {
            if check_date(d).is_ok() && s.active_on(d, weekday(d)) {
                dates.insert(d);
            }
        }
    }
    Ok(dates.into_iter().collect())
}

/// Whether Java's `LocalDate.parse(Integer.toString(d), BASIC_ISO_DATE)`
/// reads `d`: eight digits making a calendar date; the date.
fn basic_iso_date(d: i32) -> Option<u32> {
    u32::try_from(d)
        .ok()
        .filter(|&d| (10_000_101..=99_991_231).contains(&d) && check_date(d).is_ok())
}

/// The day after a valid `yyyymmdd`.
fn next_day(d: u32) -> u32 {
    if check_date(d + 1).is_ok() {
        d + 1
    } else if (d / 100) % 100 < 12 {
        (d / 100 + 1) * 100 + 1
    } else {
        (d / 10000 + 1) * 10000 + 101
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_day_rolls_over_months_and_years() {
        assert_eq!(next_day(20190131), 20190201);
        assert_eq!(next_day(20190228), 20190301);
        assert_eq!(next_day(20200228), 20200229);
        assert_eq!(next_day(20191231), 20200101);
        assert_eq!(next_day(20190513), 20190514);
    }

    #[test]
    fn hash_set_order_of_small_and_large_keys() {
        // below the capacity the order is ascending
        assert_eq!(java_hash_set_order(vec![5, 3, 9, 0]), vec![0, 3, 5, 9]);
        // 16 and 0 share bucket 0 of a 16-slot table: insertion order
        assert_eq!(java_hash_set_order(vec![16, 1, 0]), vec![16, 0, 1]);
        // a key above 2^16 is spread by its high half
        let k = (1 << 16) | 2; // (k ^ k >> 16) & 15 = 3
        assert_eq!(java_hash_set_order(vec![k, 3, 2]), vec![2, k, 3]);
    }

    #[test]
    fn transit_modes_as_r5() {
        assert_eq!(transit_mode(3), Some("BUS"));
        assert_eq!(transit_mode(2), Some("RAIL"));
        assert_eq!(transit_mode(700), Some("BUS"));
        assert_eq!(transit_mode(1100), Some("AIR"));
        assert_eq!(transit_mode(5), Some("CABLE_CAR"));
        assert_eq!(transit_mode(8), None);
        assert_eq!(transit_mode(1500), None);
    }

    const SPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../r/inst/extdata/spo");

    #[test]
    fn network_tables_of_the_sample() {
        let (net, layer) = crate::build_from_dir(SPO, &crate::BuildOptions::default()).unwrap();

        // every point of the grid snaps within 300 m, on the streets' grid
        let pts = [(-23.5563, -46.6290), (-23.5577, -46.6321), (-10.0, -10.0)];
        let snaps = find_snap(&net, &pts, 1600.0, Mode::Walk).unwrap();
        for s in snaps[..2].iter().map(|s| s.unwrap()) {
            assert!(s.distance_m < 300.0, "{s:?}");
            assert_eq!((s.lat * 1e7).round() / 1e7, s.lat);
        }
        assert_eq!(snaps[2], None);
        assert!(find_snap(&net, &pts, -1.0, Mode::Walk).is_err());
        assert!(find_snap(&net, &pts, 0.0, Mode::Walk)
            .unwrap()
            .iter()
            .all(Option::is_none));

        // the box covers every vertex of the edges
        let [xmin, ymin, xmax, ymax] = net.bbox();
        let v = street_vertices(&net);
        assert!(!v.index.is_empty());
        for (&lat, &lon) in v.lat.iter().zip(&v.lon) {
            assert!(xmin <= lon && lon <= xmax && ymin <= lat && lat <= ymax);
        }
        let mut sorted = v.index.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), v.index.len());

        let stops = transit_stops(&net, &layer);
        assert_eq!(stops.stop_id.len(), layer.n_stops());
        assert!(stops.stop_name.iter().all(Option::is_some));
        for i in 0..stops.stop_id.len() {
            assert_eq!(stops.lat[i].is_some(), stops.linked_to_street[i]);
        }
        let routes = transit_routes(&net, &layer).unwrap();
        assert_eq!(routes.len(), layer.n_patterns());
        assert!(routes
            .agency_name
            .iter()
            .all(|a| a.as_deref() == Some("SPTRANS")));
        // no shapes.txt in the sample: the lines go through the linked stops
        // (the crop leaves some stops of a route outside the streets)
        for (i, p) in layer.patterns.iter().enumerate() {
            let linked = p
                .stops
                .iter()
                .filter(|&&s| layer.stop_vertex[s as usize] >= 0)
                .count();
            assert_eq!(routes.geometry(i).len(), linked);
        }
        assert!((0..routes.len()).any(|i| routes.geometry(i).len() >= 2));

        let dates = transit_service_dates(&layer).unwrap();
        assert!(dates.windows(2).all(|w| w[0] < w[1]));
        assert!(dates.contains(&20190513));
        for &d in &dates[..10] {
            assert!(layer.n_services_active_on(d).unwrap() > 0);
        }
    }
}
