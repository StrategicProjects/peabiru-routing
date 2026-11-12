//! r5r's `pareto_frontier()` (`ParetoFrontierCalculator`): for every fare
//! cutoff, the travel-time matrix of R5's fare router with `maxFare` set to
//! the cutoff, then per destination and percentile the cutoffs at which
//! the travel time changes.

use super::matrix::{max_fare_cents, par_origins, percentile_index, Query};
use super::raptor::UNREACHED;
use super::TransitLayer;
use crate::street::router::Mode;
use crate::street::StreetNetwork;
use crate::transit::matrix::TransitData;
use crate::{RouteError, RouteParams, TransitParams};

/// One row of a [`ParetoFrontier`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct ParetoRow {
    /// Index into the origins.
    pub origin: usize,
    /// Index into the destinations.
    pub destination: usize,
    /// The travel-time percentile.
    pub percentile: i32,
    /// Whole minutes, below `max_trip_duration`.
    pub travel_time: i32,
    /// The fare cutoff at which this travel time is first reached, as the
    /// double of the Java `float` r5r reads it into (`4.8` is
    /// `4.800000190734863`).
    pub monetary_cost: f64,
}

/// r5r's `pareto_frontier()` result: rows origin by origin, destination by
/// destination, percentile by percentile (ascending), then in the order of
/// the fare cutoffs.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ParetoFrontier {
    /// The rows.
    pub rows: Vec<ParetoRow>,
}

/// The work of [`crate::pareto_frontier`].
pub(crate) fn pareto_frontier(
    net: &StreetNetwork,
    transit: Option<(&TransitLayer, &TransitData)>,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
    fare_cutoffs: &[f64],
) -> Result<ParetoFrontier, RouteError> {
    let Some(fs) = &p.fare_structure else {
        return Err(RouteError::Invalid(
            "fare_structure is required; create one with setup_fare_structure() or \
             read_fare_structure()"
                .into(),
        ));
    };
    if fare_cutoffs.is_empty() || fare_cutoffs.iter().any(|c| c.is_nan() || *c < 0.0) {
        return Err(RouteError::Invalid(
            "fare_cutoffs must hold at least one number, each >= 0".into(),
        ));
    }
    for (k, c) in fare_cutoffs.iter().enumerate() {
        if fare_cutoffs[..k].contains(c) {
            return Err(RouteError::Invalid(format!(
                "fare_cutoffs must be unique, {c} is repeated"
            )));
        }
    }
    // rJava::.jfloat(fare_cutoffs); the row value is (double) of the float
    let cutoffs: Vec<f32> = fare_cutoffs.iter().map(|&c| c as f32).collect();
    let cents: Vec<i32> = cutoffs
        .iter()
        .map(|&c| max_fare_cents(f64::from(c)))
        .collect();
    let mut p = p.clone();
    p.draws_per_minute = 1;
    p.max_fare = f64::INFINITY;
    let mut percentiles = p.percentiles.clone();
    percentiles.sort_unstable();
    let np = percentiles.len();
    let nd = destinations.len();
    let max_min = p.max_trip_duration;
    // per origin: minutes[cutoff][destination * np + k]
    let per_origin: Vec<Vec<Vec<i32>>> = match transit {
        Some((layer, data)) => {
            p.check().map_err(RouteError::Invalid)?;
            let q = Query::new(net, layer, data, destinations, &p)?;
            let rows = par_origins(p.n_threads, origins.len(), &p.cancel, |i, scratch| {
                let mut out = Vec::with_capacity(cents.len());
                for &c in &cents {
                    let mut row = vec![UNREACHED; nd * np];
                    q.route_origin_fare(origins[i], scratch, c, |j, tt| {
                        tt.sort_unstable();
                        let n = tt.len();
                        for (k, &pc) in percentiles.iter().enumerate() {
                            let v = tt[percentile_index(pc, n).min(n - 1)];
                            row[j * np + k] = if v == UNREACHED { UNREACHED } else { v / 60 };
                        }
                    })?;
                    out.push(row);
                }
                Ok::<_, RouteError>(out)
            })?;
            rows.into_iter().collect::<Result<_, _>>()?
        }
        None => {
            // the fare structure is still read in, as r5r does
            crate::fares::FareStructure::check(fs).map_err(RouteError::Invalid)?;
            crate::transit::matrix::check_percentiles(&p.percentiles)
                .map_err(RouteError::Invalid)?;
            let rp = RouteParams {
                walk_speed: p.walk_speed,
                bike_speed: p.bike_speed,
                max_lts: p.max_lts,
                max_trip_duration: p.max_trip_duration,
                n_threads: p.n_threads,
                cancel: p.cancel.clone(),
                ..RouteParams::default()
            };
            let mode: Mode = p.mode;
            let m = crate::street::router::street_travel_time_matrix(
                net,
                origins,
                destinations,
                mode,
                &rp,
            )?;
            (0..origins.len())
                .map(|i| {
                    let row: Vec<i32> = (0..nd)
                        .flat_map(|j| {
                            let v = m.get(i, j).map_or(UNREACHED, |v| v as i32);
                            std::iter::repeat(v).take(np)
                        })
                        .collect();
                    vec![row; cents.len()]
                })
                .collect()
        }
    };
    let mut rows = Vec::new();
    for (i, by_cutoff) in per_origin.iter().enumerate() {
        for j in 0..nd {
            for (k, &pc) in percentiles.iter().enumerate() {
                let mut previous = -1;
                for (c, &cutoff) in cutoffs.iter().enumerate() {
                    let tt = by_cutoff[c][j * np + k];
                    if tt != previous && tt < max_min {
                        rows.push(ParetoRow {
                            origin: i,
                            destination: j,
                            percentile: pc,
                            travel_time: tt,
                            monetary_cost: f64::from(cutoff),
                        });
                        previous = tt;
                    }
                }
            }
        }
    }
    Ok(ParetoFrontier { rows })
}
