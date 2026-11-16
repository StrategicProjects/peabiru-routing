//! Straight-line distances between two sets of points, for the location
//! models where the service standard is a distance.

use std::collections::HashMap;

use crate::fastmath_trig::haversine_m;
use crate::{CancelToken, RouteError};

/// The sphere of [`haversine_m`], in metres.
const RADIUS_M: f64 = 6_367_450.0;

/// The pairs of [`distance_pairs`]: point `from[k]` of the first set is
/// `metres[k]` from point `to[k]` of the second, ordered by `from`, then
/// by `to`.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct DistancePairs {
    /// The index of the point of the first set.
    pub from: Vec<u32>,
    /// The index of the point of the second set.
    pub to: Vec<u32>,
    /// The great-circle distance between them, in metres.
    pub metres: Vec<f64>,
}

/// Every pair of a point of `from` and a point of `to` at most
/// `max_distance` metres apart in a straight line, with the distance.
///
/// The points are `(lon, lat)` in degrees and the distance is the
/// great-circle one on a sphere of radius 6,367,450 m, the same that
/// measures the streets of a network. A pair at distance 0 is a pair. The
/// result goes into [`CostTable::from_pairs`](crate::CostTable::from_pairs)
/// as it is, the metres as the cost.
///
/// Errors: [`RouteError::Invalid`] for a coordinate that is not finite, a
/// latitude outside -90 to 90, a `max_distance` that is negative or not
/// finite, or more than `u32::MAX` points in a set;
/// [`RouteError::Cancelled`].
pub fn distance_pairs(
    from: &[(f64, f64)],
    to: &[(f64, f64)],
    max_distance: f64,
    cancel: &CancelToken,
) -> Result<DistancePairs, RouteError> {
    let invalid = |m: String| Err(RouteError::Invalid(m));
    if !(max_distance.is_finite() && max_distance >= 0.0) {
        return invalid(format!(
            "max_distance must be finite and not negative, got {max_distance}"
        ));
    }
    for (name, points) in [("origin", from), ("destination", to)] {
        if points.len() > u32::MAX as usize {
            return invalid(format!("at most 4294967295 {name}s, got {}", points.len()));
        }
        if let Some(k) = points
            .iter()
            .position(|p| !(p.0.is_finite() && p.1.is_finite() && p.1.abs() <= 90.0))
        {
            return invalid(format!(
                "{name} {k} has no valid coordinates: lon {}, lat {}",
                points[k].0, points[k].1
            ));
        }
    }
    let mut out = DistancePairs::default();
    if from.is_empty() || to.is_empty() {
        return Ok(out);
    }

    // a grid of rows of latitude as tall as the distance, each cut in
    // columns as wide as the distance is at the row's highest latitude
    let angle = max_distance / RADIUS_M;
    let slack = 1.0 + 1e-9;
    let row_height = (angle.to_degrees() * slack).clamp(1e-7, 180.0);
    let n_rows = (180.0 / row_height).ceil().max(1.0) as i64;
    let row_of = |lat: f64| (((lat + 90.0) / row_height).floor() as i64).clamp(0, n_rows - 1);
    // the half-width in degrees of longitude of the circle around a point
    let reach = |lat: f64| -> f64 {
        let lat = lat.abs().to_radians();
        if angle + lat >= std::f64::consts::FRAC_PI_2 {
            return 180.0;
        }
        ((angle.sin() / lat.cos()).min(1.0).asin().to_degrees() * slack + 1e-9).min(180.0)
    };
    let columns_of = |row: i64| -> i64 {
        let low = -90.0 + row as f64 * row_height;
        let highest = low.abs().max((low + row_height).abs()).min(90.0);
        ((360.0 / reach(highest).max(1e-7)).floor() as i64).clamp(1, 1 << 24)
    };
    let wrap = |lon: f64| (lon + 180.0).rem_euclid(360.0);
    let mut cells: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
    for (j, &(lon, lat)) in to.iter().enumerate() {
        let row = row_of(lat);
        let n = columns_of(row);
        let col = ((wrap(lon) / 360.0 * n as f64).floor() as i64).clamp(0, n - 1);
        cells.entry((row, col)).or_default().push(j as u32);
    }

    let mut near: Vec<(u32, f64)> = Vec::new();
    for (i, &(lon, lat)) in from.iter().enumerate() {
        if i % 1024 == 0 {
            cancel.check()?;
        }
        near.clear();
        let row = row_of(lat);
        let half = reach(lat);
        for r in (row - 1).max(0)..=(row + 1).min(n_rows - 1) {
            let n = columns_of(r);
            let width = 360.0 / n as f64;
            let first = ((wrap(lon) - half) / width).floor() as i64;
            let last = ((wrap(lon) + half) / width).floor() as i64;
            let (first, last) = if last - first + 1 >= n {
                (0, n - 1)
            } else {
                (first, last)
            };
            for c in first..=last {
                let Some(cell) = cells.get(&(r, c.rem_euclid(n))) else {
                    continue;
                };
                for &j in cell {
                    let (lon1, lat1) = to[j as usize];
                    let d = haversine_m(lat, lon, lat1, lon1);
                    if d <= max_distance {
                        near.push((j, d));
                    }
                }
            }
        }
        near.sort_unstable_by_key(|p| p.0);
        for &(j, d) in &near {
            out.from.push(i as u32);
            out.to.push(j);
            out.metres.push(d);
        }
    }
    Ok(out)
}
