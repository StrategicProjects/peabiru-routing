//! gtfs-lib's test of whether a stop is in a populated place
//! (`BooleanAsciiGrid.forEarthPopulation`, used by
//! `PostLoadValidator.validateStopPopulationDensity`).
//!
//! The grid is gtfs-lib's `gpwv3-quarter-buffer-boolean.asc`: quarter-degree
//! cells from 58 degrees south to 85 north where the population density of
//! the cell or of a neighbouring one is above 5 people per square
//! kilometre, from the Gridded Population of the World, version 3 (Center
//! for International Earth Science Information Network - CIESIN, Columbia
//! University; Creative Commons Attribution licence), as shipped in R5. It
//! is kept here as its bits (cell `i` in bit `i % 8` of byte `i / 8`, rows
//! from the north), compressed with DEFLATE.

use std::sync::OnceLock;

const NCOLS: i32 = 1440;
const NROWS: i32 = 572;
const XLLCORNER: f64 = -180.0;
const YLLCORNER: f64 = -58.0;
const CELLSIZE: f64 = 0.25;

static BITS: &[u8] = include_bytes!("gpwv3-quarter-buffer-boolean.bits.deflate");

/// The population grid.
pub(crate) struct Grid {
    bits: Vec<u8>,
    /// Java `BitSet.length()`: the highest set bit plus one.
    length: i32,
}

impl Grid {
    /// The grid of the whole Earth, decompressed once.
    pub fn earth() -> &'static Grid {
        static GRID: OnceLock<Grid> = OnceLock::new();
        GRID.get_or_init(|| {
            let bits = miniz_oxide::inflate::decompress_to_vec(BITS).expect("embedded grid");
            debug_assert_eq!(bits.len(), (NCOLS * NROWS) as usize / 8);
            let length = (0..bits.len() * 8)
                .rev()
                .find(|&i| bits[i / 8] >> (i % 8) & 1 == 1)
                .map_or(0, |i| i as i32 + 1);
            Grid { bits, length }
        })
    }

    /// `getValueForCoords(x, y)`, with Java's arithmetic: `(int)` casts
    /// that truncate (NaN to 0, saturating), wrapping `int` products, and
    /// the bound check against `BitSet.length()`.
    pub fn populated(&self, x: f64, y: f64) -> bool {
        let x_cell = ((x - XLLCORNER) / CELLSIZE) as i32;
        let y_cell = ((y - YLLCORNER) / CELLSIZE) as i32;
        let index = NROWS
            .wrapping_sub(y_cell)
            .wrapping_sub(1)
            .wrapping_mul(NCOLS)
            .wrapping_add(x_cell);
        if index < 0 || index > self.length {
            return false;
        }
        let i = index as usize;
        self.bits.get(i / 8).is_some_and(|b| b >> (i % 8) & 1 == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cities_are_populated_and_oceans_not() {
        let g = Grid::earth();
        assert!(g.populated(-51.22, -30.03)); // Porto Alegre
        assert!(g.populated(-46.63, -23.55)); // Sao Paulo
        assert!(!g.populated(-30.0, -30.0)); // South Atlantic
        assert!(!g.populated(0.0, 0.0)); // null island
        assert!(!g.populated(f64::NAN, f64::NAN));
        assert!(!g.populated(0.0, -80.0)); // south of the grid
    }
}
