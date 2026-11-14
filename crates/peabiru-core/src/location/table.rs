//! The table of travel times the location models work on.

use crate::RouteError;

/// Minutes from each demand point to each site (a candidate location or an
/// existing facility), the input of [`locate_facilities`](crate::locate_facilities)
/// and [`floating_catchment`](crate::floating_catchment).
///
/// Only the pairs that are reachable are held: a pair that is not in the
/// table is unreachable, so a table limited to the trips of up to some
/// duration (a matrix computed with that maximum trip duration) is as
/// good as a full one for a model that looks no farther. Demand points are
/// the rows, `0..n_demand`, and sites the columns, `0..n_sites`.
#[derive(Debug, Clone, PartialEq)]
pub struct CostTable {
    n_demand: usize,
    n_sites: usize,
    /// Per site, where its pairs start in `site_demand` / `site_minutes`.
    site_start: Vec<usize>,
    /// The demand points of each site, ascending.
    site_demand: Vec<u32>,
    site_minutes: Vec<f64>,
    /// The largest time held (0 for an empty table).
    max_minutes: f64,
}

impl CostTable {
    /// A table from its reachable pairs: demand point `demand[k]` is
    /// `minutes[k]` from site `site[k]`. A pair whose time is NaN or
    /// infinite is taken as unreachable and left out.
    ///
    /// Errors: slices of different lengths, an index outside
    /// `0..n_demand` or `0..n_sites`, a negative time, a pair given twice,
    /// or more than `u32::MAX` demand points or sites.
    pub fn from_pairs(
        n_demand: usize,
        n_sites: usize,
        demand: &[u32],
        site: &[u32],
        minutes: &[f64],
    ) -> Result<CostTable, RouteError> {
        let invalid = |m: String| Err(RouteError::Invalid(m));
        if demand.len() != site.len() || demand.len() != minutes.len() {
            return invalid(format!(
                "the pairs of a cost table need as many demand points, sites and times, got {}, {} and {}",
                demand.len(),
                site.len(),
                minutes.len()
            ));
        }
        if n_demand > u32::MAX as usize
            || n_sites > u32::MAX as usize
            || demand.len() > u32::MAX as usize
        {
            return invalid(
                "a cost table holds at most 4294967295 demand points, sites and pairs".into(),
            );
        }
        let mut count = vec![0usize; n_sites + 1];
        for k in 0..demand.len() {
            let (i, j, t) = (demand[k] as usize, site[k] as usize, minutes[k]);
            if i >= n_demand {
                return invalid(format!(
                    "pair {k} of the cost table names demand point {i}, but there are {n_demand}"
                ));
            }
            if j >= n_sites {
                return invalid(format!(
                    "pair {k} of the cost table names site {j}, but there are {n_sites}"
                ));
            }
            if t < 0.0 {
                return invalid(format!(
                    "travel times cannot be negative, got {t} for demand point {i} and site {j}"
                ));
            }
            if t.is_finite() {
                count[j + 1] += 1;
            }
        }
        for j in 0..n_sites {
            count[j + 1] += count[j];
        }
        let site_start = count.clone();
        // by demand point first, so that each site's pairs come out in
        // ascending order of demand point
        let mut order: Vec<u32> = (0..demand.len() as u32)
            .filter(|&k| minutes[k as usize].is_finite())
            .collect();
        order.sort_by_key(|&k| demand[k as usize]);
        let total = order.len();
        let mut site_demand = vec![0u32; total];
        let mut site_minutes = vec![0f64; total];
        let mut next = count;
        let mut max_minutes = 0f64;
        for k in order {
            let k = k as usize;
            let at = next[site[k] as usize];
            next[site[k] as usize] += 1;
            site_demand[at] = demand[k];
            site_minutes[at] = minutes[k];
            max_minutes = max_minutes.max(minutes[k]);
        }
        for j in 0..n_sites {
            let rows = &site_demand[site_start[j]..site_start[j + 1]];
            if let Some(w) = rows.windows(2).find(|w| w[0] == w[1]) {
                return invalid(format!(
                    "the cost table gives demand point {} and site {j} more than once",
                    w[0]
                ));
            }
        }
        Ok(CostTable {
            n_demand,
            n_sites,
            site_start,
            site_demand,
            site_minutes,
            max_minutes,
        })
    }

    /// A table from a full matrix, demand point by demand point
    /// (`minutes[demand * n_sites + site]`); NaN and infinite times are
    /// unreachable pairs.
    ///
    /// Errors: a slice that is not `n_demand * n_sites` long, or a negative
    /// time.
    pub fn from_dense(
        n_demand: usize,
        n_sites: usize,
        minutes: &[f64],
    ) -> Result<CostTable, RouteError> {
        if n_demand.checked_mul(n_sites) != Some(minutes.len()) {
            return Err(RouteError::Invalid(format!(
                "a full cost table of {n_demand} demand points and {n_sites} sites needs {} times, got {}",
                n_demand.saturating_mul(n_sites),
                minutes.len()
            )));
        }
        if n_demand > u32::MAX as usize || n_sites > u32::MAX as usize {
            return Err(RouteError::Invalid(
                "a cost table holds at most 4294967295 demand points and sites".into(),
            ));
        }
        let (mut demand, mut site, mut times) = (Vec::new(), Vec::new(), Vec::new());
        for (k, &t) in minutes.iter().enumerate() {
            if t.is_finite() {
                demand.push((k / n_sites) as u32);
                site.push((k % n_sites) as u32);
                times.push(t);
            }
        }
        CostTable::from_pairs(n_demand, n_sites, &demand, &site, &times)
    }

    /// A table from the whole minutes of a travel-time matrix, demand
    /// point by demand point (`minutes[demand * n_sites + site]`, `None`
    /// for an unreachable pair): the slice a
    /// [`StreetMatrix`](crate::StreetMatrix) gives with `as_slice()`
    /// when the demand points are its origins and the sites its
    /// destinations, or a [`TransitMatrix`](crate::TransitMatrix) computed
    /// for one percentile.
    ///
    /// Errors: a slice that is not `n_demand * n_sites` long.
    pub fn from_minutes(
        n_demand: usize,
        n_sites: usize,
        minutes: &[Option<u32>],
    ) -> Result<CostTable, RouteError> {
        let dense: Vec<f64> = minutes
            .iter()
            .map(|m| m.map_or(f64::NAN, f64::from))
            .collect();
        CostTable::from_dense(n_demand, n_sites, &dense)
    }

    /// Number of demand points (rows).
    pub fn n_demand(&self) -> usize {
        self.n_demand
    }

    /// Number of sites (columns).
    pub fn n_sites(&self) -> usize {
        self.n_sites
    }

    /// Number of reachable pairs held.
    pub fn n_pairs(&self) -> usize {
        self.site_demand.len()
    }

    /// The minutes from a demand point to a site, `None` for an
    /// unreachable pair.
    ///
    /// # Panics
    ///
    /// If either index is outside the table.
    pub fn get(&self, demand: usize, site: usize) -> Option<f64> {
        assert!(demand < self.n_demand, "demand point outside the table");
        let (rows, times) = self.site(site);
        rows.binary_search(&(demand as u32)).ok().map(|k| times[k])
    }

    /// The largest time held (0 for a table without pairs).
    pub fn max_minutes(&self) -> f64 {
        self.max_minutes
    }

    /// The demand points that reach site `j`, ascending, and their times.
    pub(crate) fn site(&self, j: usize) -> (&[u32], &[f64]) {
        let (a, b) = (self.site_start[j], self.site_start[j + 1]);
        (&self.site_demand[a..b], &self.site_minutes[a..b])
    }
}
