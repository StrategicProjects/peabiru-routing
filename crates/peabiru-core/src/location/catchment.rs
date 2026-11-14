//! Two-step floating catchment area: supply against demand around each
//! place.

use super::CostTable;
use crate::transit::decay::Decay;
use crate::{CancelToken, DecayFunction, RouteError};

/// How [`floating_catchment`] weights travel time. Fill it in by changing
/// the fields of [`FloatingCatchmentParams::default()`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FloatingCatchmentParams {
    /// The catchment in minutes. With [`DecayFunction::Step`] a pair
    /// counts in full up to it (`time <= cutoff`) and not at all beyond;
    /// the other functions take it as the accessibility functions do (the
    /// half-life of `Exponential`, the centre of `Linear`, the median of
    /// `Logistic`), and `FixedExponential` ignores it. No default: it must
    /// be set, finite and positive, except for `FixedExponential`.
    pub cutoff: f64,
    /// The weight of a pair by its travel time (default
    /// [`DecayFunction::Step`], the two-step floating catchment area in
    /// its original form). Any other function gives the enhanced form,
    /// with the weight falling continuously with travel time: every pair
    /// of the table counts with its weight, so the table should reach as
    /// far as the weights matter.
    pub decay: DecayFunction,
    /// Cancels the computation: checked between facilities.
    pub cancel: CancelToken,
}

impl Default for FloatingCatchmentParams {
    fn default() -> Self {
        FloatingCatchmentParams {
            cutoff: f64::NAN,
            decay: DecayFunction::Step,
            cancel: CancelToken::default(),
        }
    }
}

/// The result of [`floating_catchment`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct FloatingCatchment {
    /// Per demand point, the accessibility index: the sum, over the
    /// facilities it reaches, of the facility's ratio times the weight of
    /// the pair. Supply per unit of demand (places per resident, say).
    pub index: Vec<f64>,
    /// Per facility, its supply divided by the demand in its catchment
    /// (0 when the catchment holds no demand).
    pub ratio: Vec<f64>,
    /// Per facility, the demand in its catchment: the sum of the demand
    /// weights times the weight of each pair.
    pub catchment_demand: Vec<f64>,
    /// The supply of the facilities whose catchment holds no demand. The
    /// rest of the supply is what the index shares out: the sum of demand
    /// times index equals the total supply less this.
    pub unreached_supply: f64,
}

/// The two-step floating catchment area of Luo and Wang (2003) and, with a
/// decay function, its enhanced form (Luo and Qi, 2009).
///
/// Step 1: each facility's supply is divided by the demand within its
/// catchment, each demand point weighted by the decay of its travel time.
/// Step 2: each demand point adds up the ratios of the facilities within
/// its catchment, weighted the same way. `costs` gives the minutes from
/// each demand point (row) to each facility (column), `demand` the weight
/// of each demand point and `supply` the capacity of each facility.
///
/// Errors: [`RouteError::Invalid`] for weights or capacities that do not
/// match the table, are negative or are not finite, a cutoff that is not
/// set, or a decay parameter out of range; [`RouteError::Cancelled`].
pub fn floating_catchment(
    costs: &CostTable,
    demand: &[f64],
    supply: &[f64],
    params: &FloatingCatchmentParams,
) -> Result<FloatingCatchment, RouteError> {
    let invalid = |m: String| Err(RouteError::Invalid(m));
    if demand.len() != costs.n_demand() {
        return invalid(format!(
            "demand has {} weights, but the table has {} demand points",
            demand.len(),
            costs.n_demand()
        ));
    }
    if supply.len() != costs.n_sites() {
        return invalid(format!(
            "supply has {} capacities, but the table has {} facilities",
            supply.len(),
            costs.n_sites()
        ));
    }
    if let Some(i) = demand.iter().position(|w| !(w.is_finite() && *w >= 0.0)) {
        return invalid(format!(
            "demand weights must be finite and not negative, got {} for demand point {i}",
            demand[i]
        ));
    }
    if let Some(j) = supply.iter().position(|w| !(w.is_finite() && *w >= 0.0)) {
        return invalid(format!(
            "supply must be finite and not negative, got {} for facility {j}",
            supply[j]
        ));
    }
    let fixed = matches!(params.decay, DecayFunction::FixedExponential { .. });
    let cutoff_max = f64::from(i32::MAX / 60);
    if !fixed && !(params.cutoff > 0.0 && params.cutoff <= cutoff_max) {
        return invalid(format!(
            "floating_catchment needs a cutoff in minutes, finite and positive, got {}",
            params.cutoff
        ));
    }
    let decay = Decay::new(params.decay.clone()).map_err(RouteError::Invalid)?;
    let step = matches!(params.decay, DecayFunction::Step);
    let cutoff_seconds = if fixed {
        0
    } else {
        (params.cutoff * 60.0).round() as i32
    };
    let weight = |minutes: f64| -> f64 {
        if step {
            f64::from(u8::from(minutes <= params.cutoff))
        } else {
            // `as` saturates: a time beyond i32 seconds weighs nothing
            decay.weight(cutoff_seconds, (minutes * 60.0).round() as i32)
        }
    };

    let n_sites = costs.n_sites();
    let mut ratio = vec![0f64; n_sites];
    let mut catchment_demand = vec![0f64; n_sites];
    let mut index = vec![0f64; costs.n_demand()];
    let mut unreached_supply = 0f64;
    for j in 0..n_sites {
        params.cancel.check()?;
        let (rows, times) = costs.site(j);
        let around: f64 = rows
            .iter()
            .zip(times)
            .map(|(&i, &t)| demand[i as usize] * weight(t))
            .sum();
        catchment_demand[j] = around;
        if around > 0.0 {
            ratio[j] = supply[j] / around;
            for (&i, &t) in rows.iter().zip(times) {
                index[i as usize] += ratio[j] * weight(t);
            }
        } else {
            unreached_supply += supply[j];
        }
    }
    Ok(FloatingCatchment {
        index,
        ratio,
        catchment_demand,
        unreached_supply,
    })
}
