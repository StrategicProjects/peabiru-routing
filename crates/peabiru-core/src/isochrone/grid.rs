//! The Web Mercator grid of travel-time surfaces (R5 `WebMercatorExtents`,
//! `WebMercatorGridPointSet` and the pixel functions of `Grid`).

use crate::street::geom::FIXED_FACTOR;
use crate::street::StreetNetwork;
use crate::RouteError;

/// Lowest and highest zoom level R5 accepts (`WebMercatorExtents.MIN_ZOOM`,
/// `MAX_ZOOM`).
pub(crate) const MIN_ZOOM: i32 = 9;
/// See [`MIN_ZOOM`].
pub(crate) const MAX_ZOOM: i32 = 12;
/// `WebMercatorExtents.MAX_GRID_CELLS`.
const MAX_GRID_CELLS: i64 = 5_000_000;

/// `Grid.lonToFractionalPixel`.
pub(crate) fn lon_to_fractional_pixel(lon: f64, zoom: i32) -> f64 {
    (lon + 180.0) / 360.0 * 2f64.powi(zoom) * 256.0
}

/// `Grid.latToFractionalPixel`.
pub(crate) fn lat_to_fractional_pixel(lat: f64, zoom: i32) -> f64 {
    let lat_rad = lat.to_radians();
    (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI)
        * 2f64.powi(zoom - 1)
        * 256.0
}

/// `Grid.lonToPixel`: Java `(int)`.
pub(crate) fn lon_to_pixel(lon: f64, zoom: i32) -> i32 {
    lon_to_fractional_pixel(lon, zoom) as i32
}

/// `Grid.latToPixel`: Java `(int)`.
pub(crate) fn lat_to_pixel(lat: f64, zoom: i32) -> i32 {
    lat_to_fractional_pixel(lat, zoom) as i32
}

/// `Grid.pixelToLon` of a pixel's centre (`pixelToCenterLon`).
fn pixel_center_lon(x: i32, zoom: i32) -> f64 {
    (f64::from(x) + 0.5) / (2f64.powi(zoom) * 256.0) * 360.0 - 180.0
}

/// `Grid.pixelToLat` of a pixel's centre (`pixelToCenterLat`).
fn pixel_center_lat(y: i32, zoom: i32) -> f64 {
    use std::f64::consts::PI;
    let tile = (f64::from(y) + 0.5) / 256.0;
    (PI - tile * PI * 2.0 / 2f64.powi(zoom))
        .sinh()
        .atan()
        .to_degrees()
}

/// A block of Web Mercator pixels at one zoom level: the destinations of a
/// travel-time surface. Pixel numbers are
/// absolute (world) numbers at the zoom level, `x` growing eastward and
/// `y` southward; cell `i` is pixel `(west + i % width, north + i / width)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebMercatorGrid {
    zoom: i32,
    west: i32,
    north: i32,
    width: i32,
    height: i32,
}

impl WebMercatorGrid {
    /// The grid polygon isochrones are interpolated from: the pixels
    /// covering the street network's envelope (the
    /// envelope holds every OpenStreetMap node of the ways streets were
    /// made from). `zoom` must be 9 to 12, and the grid at most 5,000,000
    /// cells; [`RouteError::Invalid`] otherwise, and for a
    /// network without streets.
    pub fn for_network(network: &crate::Network, zoom: i32) -> Result<Self, RouteError> {
        Self::for_street(network.street(), zoom)
    }

    /// [`WebMercatorGrid::for_network`] of a street network.
    pub(crate) fn for_street(net: &StreetNetwork, zoom: i32) -> Result<Self, RouteError> {
        check_zoom(zoom)?;
        let Some((min_lat, max_lat, min_lon, max_lon)) = net.envelope else {
            return Err(RouteError::Invalid(
                "the network has no streets to make a grid of".into(),
            ));
        };
        let deg = |f: i32| f64::from(f) / FIXED_FACTOR;
        let north = lat_to_pixel(deg(max_lat), zoom);
        let west = lon_to_pixel(deg(min_lon), zoom);
        let height = (lat_to_fractional_pixel(deg(min_lat), zoom) - f64::from(north)).ceil() as i32;
        let width = (lon_to_fractional_pixel(deg(max_lon), zoom) - f64::from(west)).ceil() as i32;
        Self::new(west, north, width, height, zoom)
    }

    /// A grid of `width` x `height` pixels whose north-west pixel is
    /// `(west, north)` at `zoom`, with the checks of
    /// [`for_network`](Self::for_network).
    pub fn new(
        west: i32,
        north: i32,
        width: i32,
        height: i32,
        zoom: i32,
    ) -> Result<Self, RouteError> {
        check_zoom(zoom)?;
        if width < 1 || height < 1 {
            return Err(RouteError::Invalid(format!(
                "a grid needs a width and height of at least 1, got {width} x {height}"
            )));
        }
        let cells = i64::from(width) * i64::from(height);
        if cells > MAX_GRID_CELLS {
            return Err(RouteError::Invalid(format!(
                "Requested number of destinations ({cells}) exceeds limit ({MAX_GRID_CELLS}). \
                 Use a lower zoom level."
            )));
        }
        Ok(WebMercatorGrid {
            zoom,
            west,
            north,
            width,
            height,
        })
    }

    /// Zoom level, 9 to 12.
    pub fn zoom(&self) -> i32 {
        self.zoom
    }

    /// Pixel number of the westernmost column.
    pub fn west(&self) -> i32 {
        self.west
    }

    /// Pixel number of the northernmost row.
    pub fn north(&self) -> i32 {
        self.north
    }

    /// Number of columns.
    pub fn width(&self) -> i32 {
        self.width
    }

    /// Number of rows.
    pub fn height(&self) -> i32 {
        self.height
    }

    /// Number of cells, `width * height`.
    pub fn n_cells(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// `(lat, lon)` degrees of the centre of cell `i` (row by row from the
    /// north-west), the point routed to.
    pub fn cell_center(&self, i: usize) -> (f64, f64) {
        let w = self.width as usize;
        let x = self.west + (i % w) as i32;
        let y = self.north + (i / w) as i32;
        (
            pixel_center_lat(y, self.zoom),
            pixel_center_lon(x, self.zoom),
        )
    }

    /// Every cell centre, in cell order (see [`WebMercatorGrid::cell_center`]).
    pub fn cell_centers(&self) -> Vec<(f64, f64)> {
        (0..self.n_cells()).map(|i| self.cell_center(i)).collect()
    }

    /// The cells R5 considers for points inside an envelope given in
    /// floating degrees (`WebMercatorGridPointSet.getPointsInEnvelope`):
    /// every pixel the envelope's edges fall in, inclusive, clipped to the
    /// grid. Returns the rows and the columns of those cells, `(rows,
    /// columns)` inclusive, or `None` when the envelope misses the grid.
    pub(crate) fn cells_in_envelope(
        &self,
        (south, north, west, east): (f64, f64, f64, f64),
    ) -> Option<((usize, usize), (usize, usize))> {
        let z = self.zoom;
        let x0 = (lon_to_pixel(west, z) - self.west).max(0);
        let x1 = (lon_to_pixel(east, z) - self.west).min(self.width - 1);
        let y0 = (lat_to_pixel(north, z) - self.north).max(0);
        let y1 = (lat_to_pixel(south, z) - self.north).min(self.height - 1);
        (x0 <= x1 && y0 <= y1).then_some(((y0 as usize, y1 as usize), (x0 as usize, x1 as usize)))
    }
}

fn check_zoom(zoom: i32) -> Result<(), RouteError> {
    if (MIN_ZOOM..=MAX_ZOOM).contains(&zoom) {
        Ok(())
    } else {
        Err(RouteError::Invalid(format!(
            "zoom must be from {MIN_ZOOM} to {MAX_ZOOM}, got {zoom}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_round_trip() {
        let (lat, lon) = (pixel_center_lat(153981, 10), pixel_center_lon(93740, 10));
        assert_eq!(lat_to_pixel(lat, 10), 153981);
        assert_eq!(lon_to_pixel(lon, 10), 93740);
    }

    #[test]
    fn zoom_and_size_are_checked() {
        assert!(WebMercatorGrid::new(0, 0, 10, 10, 8).is_err());
        assert!(WebMercatorGrid::new(0, 0, 10, 10, 13).is_err());
        assert!(WebMercatorGrid::new(0, 0, 3000, 3000, 12).is_err());
        assert!(WebMercatorGrid::new(0, 0, 0, 10, 10).is_err());
        let g = WebMercatorGrid::new(5, 7, 3, 2, 9).unwrap();
        assert_eq!(g.n_cells(), 6);
        let env = |s, n, w, e| {
            (
                pixel_center_lat(s, 9),
                pixel_center_lat(n, 9),
                pixel_center_lon(w, 9),
                pixel_center_lon(e, 9),
            )
        };
        // rows 0 and 1, columns 1 and 2: cells 1, 2, 4, 5
        assert_eq!(g.cells_in_envelope(env(8, 7, 6, 9)), Some(((0, 1), (1, 2))));
        assert_eq!(g.cells_in_envelope(env(7, 7, 5, 5)), Some(((0, 0), (0, 0))));
        assert_eq!(g.cells_in_envelope(env(8, 7, 9, 12)), None);
        assert_eq!(g.cells_in_envelope(env(12, 9, 5, 7)), None);
    }
}
