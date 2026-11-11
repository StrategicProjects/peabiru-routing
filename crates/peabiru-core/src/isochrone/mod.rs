//! Isochrones, as r5r's `isochrone()`.
//!
//! Polygon isochrones (r5r's default, `polygon_output = TRUE`) are made in
//! two steps, both here: a travel-time *surface* from each origin to the
//! cells of a Web Mercator grid covering the network (R5's
//! `TravelTimeSurfaceTask`; [`street_travel_time_surface`],
//! [`transit_travel_time_surface`]), then the band of each cutoff traced
//! over the surface by marching squares and grouped into polygons with
//! holes, as the R package isoband does for r5r ([`isochrone_polygons`]).
//! The surface is R5's, value for value; the rings are isoband's, vertex
//! for vertex and in the order isoband gives them on the platform (it
//! depends on the C++ standard library isoband is built with; see
//! [`isochrone_polygons`]). r5r then hands the
//! polygons to `sf::st_make_valid()`, which on longitude/latitude data
//! rebuilds them with s2 (snapping to 1e-7 degrees); that last step is not
//! reproduced here: the R package applies it, other callers get isoband's
//! polygons.
//!
//! Line isochrones (`polygon_output = FALSE`) are the street edges reached
//! from each origin, banded by cutoff: [`street_edges`] lists the edges and
//! the centroid r5r routes to on each, a travel-time matrix to those
//! centroids gives the times, and [`isochrone_lines`] bands and orders them.

mod edges;
mod grid;
mod isoband;
pub(crate) mod linkage;
mod separate;

pub use edges::{isochrone_lines, street_edges, IsochroneLine, StreetEdges};
pub use grid::WebMercatorGrid;
pub use isoband::CxxLibrary;
pub use separate::Polygon;

use crate::street::router::{travel_time_seconds_linked, Mode, RouteParams};
use crate::street::StreetNetwork;
use crate::transit::matrix::{check_percentiles, par_origins, percentile_index, Query};
use crate::transit::raptor::UNREACHED;
use crate::{RouteError, TransitData, TransitLayer, TransitParams};

/// Travel times from each origin to every cell of a Web Mercator grid, in
/// whole minutes at each percentile, as R5's travel-time surface (and
/// r5r's internal `travel_time_surface()`): `i32::MAX` where a cell is not
/// reached. The minutes are not capped at the trip limit: a street time
/// includes the partial edges at both ends and may pass it, as in R5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TravelTimeSurface {
    grid: WebMercatorGrid,
    percentiles: Vec<i32>,
    n_origins: usize,
    /// `minutes[(origin * n_percentiles + k) * n_cells + cell]`.
    minutes: Vec<i32>,
}

impl TravelTimeSurface {
    /// The grid of the cells.
    pub fn grid(&self) -> &WebMercatorGrid {
        &self.grid
    }

    /// The percentiles, ascending.
    pub fn percentiles(&self) -> &[i32] {
        &self.percentiles
    }

    /// Number of origins.
    pub fn n_origins(&self) -> usize {
        self.n_origins
    }

    /// Minutes from `origin` to every cell at the `k`-th percentile, cell by
    /// cell (row by row from the north-west; see
    /// [`WebMercatorGrid::cell_center`]); `i32::MAX` where unreached.
    /// Panics when an index is out of range.
    pub fn minutes(&self, origin: usize, k: usize) -> &[i32] {
        assert!(
            origin < self.n_origins && k < self.percentiles.len(),
            "({origin}, {k}) is out of range"
        );
        let n = self.grid.n_cells();
        let at = (origin * self.percentiles.len() + k) * n;
        &self.minutes[at..at + n]
    }
}

/// Travel-time surface by a street mode alone (r5r's `isochrone()` with
/// `mode` one of `WALK`, `BICYCLE`, `CAR` and `polygon_output = TRUE`): one
/// travel time per cell, the same at every percentile. The grid is
/// [`WebMercatorGrid::for_network`] at `zoom` (9 to 12).
/// `params.max_trip_duration_min` is the search limit r5r hands R5 (its
/// `isochrone()` passes the largest cutoff plus 10 minutes). Points are
/// `(lat, lon)` degrees; an origin that cannot be linked reaches nothing.
/// [`RouteError::Invalid`] for invalid parameters, percentiles (see
/// [`validate_percentiles`](crate::validate_percentiles)) or zoom,
/// [`RouteError::Cancelled`] when `params.cancel` is cancelled.
///
/// Most of a first call from a few origins is linking the grid's cells to
/// the streets. The network keeps those links for later surfaces on the
/// same grid and mode (as R5 keeps its linkages; public transport
/// surfaces walk, so they share the walking ones): up to 9 grids and
/// modes, the least recently used dropped first beyond 64 MiB, at 40
/// bytes a cell (6.2 MiB for r5r's Porto Alegre sample at zoom 12, 0.4 MiB
/// at zoom 10). Clones of the network share them.
pub fn street_travel_time_surface(
    net: &StreetNetwork,
    origins: &[(f64, f64)],
    mode: Mode,
    params: &RouteParams,
    percentiles: &[i32],
    zoom: i32,
) -> Result<TravelTimeSurface, RouteError> {
    check_percentiles(percentiles).map_err(RouteError::Invalid)?;
    params.validate()?;
    no_scenario(&params.scenario)?;
    let grid = WebMercatorGrid::for_network(net, zoom)?;
    let mut percentiles = percentiles.to_vec();
    percentiles.sort_unstable();
    let np = percentiles.len();
    let n = grid.n_cells();
    let links = linkage::grid_links(net, &grid, mode, params.n_threads, &params.cancel)?;
    let rows = travel_time_seconds_linked(net, origins, &links, mode, params, |_, times| {
        let mut row = vec![UNREACHED; np * n];
        if let Some(times) = times {
            // TravelTimeReducer.recordUnvaryingTravelTimeAtTarget
            for (j, &t) in times.iter().enumerate() {
                let m = if t == UNREACHED { UNREACHED } else { t / 60 };
                for k in 0..np {
                    row[k * n + j] = m;
                }
            }
        }
        row
    })?;
    Ok(TravelTimeSurface {
        grid,
        percentiles,
        n_origins: origins.len(),
        minutes: rows.concat(),
    })
}

/// r5r's `isochrone()` has no scenario arguments, so there is nothing to
/// compare a surface with a scenario with: an error.
fn no_scenario(s: &crate::Scenario) -> Result<(), RouteError> {
    if s.is_unchanged() {
        Ok(())
    } else {
        Err(RouteError::Invalid(
            "travel-time surfaces take no scenario: r5r's isochrone() has no new_carspeeds, \
             carspeed_scale or new_lts"
                .into(),
        ))
    }
}

/// Travel-time surface by public transport (r5r's `isochrone()` with
/// public transport in `mode`, its modes, access and egress as in
/// [`TransitParams`](crate::TransitParams), and
/// `polygon_output = TRUE`), at each of `p.percentiles` over the departure
/// window. `p.max_trip_duration` is the limit r5r hands R5 (the largest
/// cutoff plus 10 minutes). Otherwise as [`street_travel_time_surface`].
pub fn transit_travel_time_surface(
    net: &StreetNetwork,
    layer: &TransitLayer,
    data: &TransitData,
    origins: &[(f64, f64)],
    p: &TransitParams,
    zoom: i32,
) -> Result<TravelTimeSurface, RouteError> {
    no_scenario(&p.scenario)?;
    let grid = WebMercatorGrid::for_network(net, zoom)?;
    let q = Query::new_grid(net, layer, data, &grid, p)?;
    let mut percentiles = p.percentiles.clone();
    percentiles.sort_unstable();
    let np = percentiles.len();
    let n = grid.n_cells();
    let rows = par_origins(p.n_threads, origins.len(), &p.cancel, |i, scratch| {
        let mut row = vec![UNREACHED; np * n];
        q.route_origin(origins[i], scratch, |j, tt| {
            // TravelTimeReducer.extractTravelTimePercentilesAndRecord
            tt.sort_unstable();
            let len = tt.len();
            for (k, &pc) in percentiles.iter().enumerate() {
                let v = tt[percentile_index(pc, len).min(len - 1)];
                row[k * n + j] = if v == UNREACHED { UNREACHED } else { v / 60 };
            }
        });
        row
    })?;
    Ok(TravelTimeSurface {
        grid,
        percentiles,
        n_origins: origins.len(),
        minutes: rows.concat(),
    })
}

/// r5r's cutoffs of `isochrone()`: `sort(unique(c(0, cutoffs)))` without
/// the 0, ascending. They must be finite and at least 0, and one must be
/// above 0; [`RouteError::Invalid`] otherwise.
pub fn isochrone_cutoffs(cutoffs: &[f64]) -> Result<Vec<f64>, RouteError> {
    if cutoffs.is_empty() {
        return Err(RouteError::Invalid(
            "cutoffs must have at least one value".into(),
        ));
    }
    if let Some(c) = cutoffs.iter().find(|c| !c.is_finite() || **c < 0.0) {
        return Err(RouteError::Invalid(format!(
            "cutoffs must be finite and at least 0, got {c}"
        )));
    }
    let mut out: Vec<f64> = cutoffs.iter().copied().filter(|&c| c > 0.0).collect();
    out.sort_by(f64::total_cmp);
    out.dedup();
    if out.is_empty() {
        return Err(RouteError::Invalid(
            "cutoffs must contain at least one value greater than 0".into(),
        ));
    }
    Ok(out)
}

/// The trip limit r5r's `isochrone()` derives from the cutoffs (the user's
/// `max_trip_duration` is ignored): `as.integer(max(cutoffs))`, minutes
/// truncated. Line isochrones route with it; polygon isochrones with it
/// plus 10. Errors as [`isochrone_cutoffs`], and when it does not fit an
/// `i32`.
pub fn isochrone_trip_limit(cutoffs: &[f64]) -> Result<i32, RouteError> {
    let top = *isochrone_cutoffs(cutoffs)?
        .last()
        .expect("at least one cutoff");
    if top >= 2_147_483_648.0 {
        return Err(RouteError::Invalid(format!(
            "the largest cutoff ({top}) is too large"
        )));
    }
    Ok(top as i32)
}

/// One polygon isochrone: the area reached from origin `origin` within
/// `isochrone` minutes at percentile `percentile`, as a multipolygon in
/// `[lon, lat]` degrees (empty when nothing is reached).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct IsochronePolygon {
    /// Index of the origin.
    pub origin: usize,
    /// The travel-time percentile.
    pub percentile: i32,
    /// The cutoff, minutes.
    pub isochrone: f64,
    /// The polygons, each a shell followed by its holes, every ring closed;
    /// as isoband's `separate_polygons()` makes them (shells as traced,
    /// holes reversed), before r5r's `sf::st_make_valid()`.
    pub polygons: Vec<Polygon>,
}

/// r5r's `webmercator_pixel_to_lon()`.
fn pixel_to_lon(px: f64, zoom: i32) -> f64 {
    px / 256.0 / 2f64.powi(zoom) * 360.0 - 180.0
}

/// r5r's `webmercator_pixel_to_lat()` (`rad2deg()` is `rad * 180 / pi`).
fn pixel_to_lat(px: f64, zoom: i32) -> f64 {
    use std::f64::consts::PI;
    (PI - px / 256.0 / 2f64.powi(zoom) * 2.0 * PI).sinh().atan() * 180.0 / PI
}

/// Polygon isochrones from a travel-time surface, as r5r's `isochrone()`
/// makes them (`surface_isochrone()`): for each origin, each percentile and
/// each cutoff, the cells reached in less than the cutoff, traced by
/// isoband's marching squares over the cell centres (`isoband::isobands()`
/// from 0 to the cutoff), converted to longitude and latitude and grouped
/// into polygons with holes (`isoband::iso_to_sfg()`).
///
/// `cutoffs` are r5r's (see [`isochrone_cutoffs`]). Every (origin,
/// percentile, cutoff) gets one result, ordered by origin, percentile
/// (ascending) and cutoff (descending), as r5r orders its rows.
/// [`RouteError::Invalid`] for bad cutoffs, and where isoband stops: two
/// rings whose relation it cannot decide.
///
/// The order of the rings in each band and the vertex each ring starts at
/// are those isoband gives where this crate runs: isoband walks a C++ hash
/// table, whose order is that of the C++ standard library it was built
/// with, libc++ on macOS and libstdc++ on Linux and Windows. This function
/// follows the target it is compiled for (libc++ on Apple targets and on
/// WebAssembly, libstdc++ elsewhere), so on each platform the polygons are
/// those r5r makes there.
pub fn isochrone_polygons(
    surface: &TravelTimeSurface,
    cutoffs: &[f64],
) -> Result<Vec<IsochronePolygon>, RouteError> {
    isochrone_polygons_in(surface, cutoffs, CxxLibrary::NATIVE)
}

/// [`isochrone_polygons`] with the rings in the order isoband gives them
/// when built with `lib` (exposed through `crate::unstable`).
pub(crate) fn isochrone_polygons_in(
    surface: &TravelTimeSurface,
    cutoffs: &[f64],
    lib: CxxLibrary,
) -> Result<Vec<IsochronePolygon>, RouteError> {
    let cutoffs = isochrone_cutoffs(cutoffs)?;
    let g = surface.grid;
    // r5r: west + seq_len(width) - 0.5, north + seq_len(height) - 0.5
    let x: Vec<f64> = (1..=g.width())
        .map(|i| f64::from(g.west() + i) - 0.5)
        .collect();
    let y: Vec<f64> = (1..=g.height())
        .map(|i| f64::from(g.north() + i) - 0.5)
        .collect();
    let mut out = Vec::new();
    let mut z = Vec::with_capacity(g.n_cells());
    for origin in 0..surface.n_origins {
        for (k, &percentile) in surface.percentiles.iter().enumerate() {
            z.clear();
            z.extend(surface.minutes(origin, k).iter().map(|&m| f64::from(m)));
            let mut bander = isoband::Isobander::new(
                isoband::Grid {
                    x: &x,
                    y: &y,
                    z: &z,
                },
                lib,
            );
            // isoband computes the bands in the order given (ascending); the
            // rows go out descending
            let mut bands = Vec::with_capacity(cutoffs.len());
            for &c in &cutoffs {
                let rings: Vec<isoband::Ring> = bander
                    .band(c)
                    .into_iter()
                    .map(|ring| {
                        ring.into_iter()
                            .map(|[px, py]| {
                                [pixel_to_lon(px, g.zoom()), pixel_to_lat(py, g.zoom())]
                            })
                            .collect()
                    })
                    .collect();
                let polygons = separate::separate_polygons(rings).map_err(RouteError::Invalid)?;
                bands.push(IsochronePolygon {
                    origin,
                    percentile,
                    isochrone: c,
                    polygons,
                });
            }
            bands.reverse();
            out.extend(bands);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoffs_follow_r5r() {
        assert_eq!(
            isochrone_cutoffs(&[30.0, 0.0, 15.0, 15.0]).unwrap(),
            vec![15.0, 30.0]
        );
        assert_eq!(isochrone_cutoffs(&[7.5]).unwrap(), vec![7.5]);
        assert!(isochrone_cutoffs(&[]).is_err());
        assert!(isochrone_cutoffs(&[0.0]).is_err());
        assert!(isochrone_cutoffs(&[-1.0, 10.0]).is_err());
        assert!(isochrone_cutoffs(&[f64::NAN]).is_err());
        assert!(isochrone_cutoffs(&[f64::INFINITY]).is_err());
        assert_eq!(isochrone_trip_limit(&[0.0, 15.0, 30.9]).unwrap(), 30);
    }

    #[test]
    fn pixels_to_degrees() {
        // the corners of the world at zoom 9
        let w = 256.0 * 512.0;
        assert_eq!(pixel_to_lon(0.0, 9), -180.0);
        assert_eq!(pixel_to_lon(w / 2.0, 9), 0.0);
        assert!((pixel_to_lat(w / 2.0, 9)).abs() < 1e-12);
        assert!((pixel_to_lat(0.0, 9) - 85.0511287798066).abs() < 1e-9);
    }
}
