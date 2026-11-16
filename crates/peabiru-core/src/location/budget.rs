//! Sharing a budget of facilities among groups (municipalities, districts)
//! from the curve of each: what its first, second, third facility adds.

use crate::RouteError;

/// How [`allocate_budget`] shares the facilities out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BudgetCriterion {
    /// The most demand covered in all: each facility goes where it adds
    /// most.
    Utilitarian,
    /// The worst-off group first: each facility goes to the group with
    /// the lowest share of its demand covered.
    Maximin,
}

impl BudgetCriterion {
    /// The criterion's name in the packages: `utilitarian` or `maximin`.
    pub fn name(self) -> &'static str {
        match self {
            BudgetCriterion::Utilitarian => "utilitarian",
            BudgetCriterion::Maximin => "maximin",
        }
    }

    /// The criterion of that name (`None` for any other text).
    pub fn from_name(name: &str) -> Option<BudgetCriterion> {
        Some(match name {
            "utilitarian" => BudgetCriterion::Utilitarian,
            "maximin" => BudgetCriterion::Maximin,
            _ => return None,
        })
    }
}

/// One group of [`allocate_budget`].
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetGroup<'a> {
    /// What each further facility adds in the group, in order: the
    /// `marginal` of a [`LocationSolution`](crate::LocationSolution) of
    /// maximal covering solved without swaps. It may not increase.
    pub marginal: &'a [f64],
    /// The demand covered in the group before any new facility.
    pub covered: f64,
    /// The demand of the group, for its share covered. NaN when unknown:
    /// the shares are then NaN and [`BudgetCriterion::Maximin`] is
    /// refused.
    pub total: f64,
}

/// What [`allocate_budget_with`] takes besides the criterion, one entry
/// per group in each list that is given. With none (the default) it is
/// [`allocate_budget`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub struct BudgetRules<'a> {
    /// How much the demand of each group counts (1 without the list),
    /// finite and above zero. With [`BudgetCriterion::Utilitarian`] the
    /// gains of a group are multiplied by its weight; with
    /// [`BudgetCriterion::Maximin`] its share covered is divided by it.
    pub weights: Option<&'a [f64]>,
    /// The priority tier of each group (0 without the list), the lowest
    /// first: a facility only goes to a group when no group of a lower
    /// tier has one left to open that adds anything.
    pub tiers: Option<&'a [u32]>,
    /// Whether each group may receive facilities (all without the list).
    /// The others get none and still count in the demand covered and in
    /// the lowest share.
    pub eligible: Option<&'a [bool]>,
}

/// How [`allocate_budget`] shared the facilities out, one entry per
/// facility in the order they were given: the answer for any smaller
/// budget is the beginning of the lists.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct BudgetAllocation {
    /// The group that gets each facility.
    pub group: Vec<usize>,
    /// What that facility adds.
    pub gain: Vec<f64>,
    /// The demand covered in all the groups after it, what was covered
    /// before included.
    pub covered: Vec<f64>,
    /// The lowest share covered among the groups after it (NaN without
    /// the totals of the groups).
    pub lowest_share: Vec<f64>,
    /// How many facilities each group got.
    pub units: Vec<usize>,
    /// The order in which the groups got their first facility: 0 for the
    /// first group to receive one, 1 for the next, `None` for a group
    /// that got none. One number per group that sums up the criterion,
    /// the weights and the tiers.
    pub priority: Vec<Option<usize>>,
}

/// Share `budget` facilities among groups, given what each further
/// facility adds in each group.
///
/// With [`BudgetCriterion::Utilitarian`] each facility goes to the group
/// where the next one adds most. As the gains of a group do not increase,
/// this covers the most demand possible with every budget up to `budget`
/// (marginal analysis: Fox, 1966). With [`BudgetCriterion::Maximin`] each
/// facility goes to the group with the lowest share of its demand covered
/// that still has a facility to open, which gives the highest possible
/// lowest share with every budget. Ties go to the larger gain, then to
/// the group listed first. Fewer than `budget` facilities come back when
/// no group has a facility left that adds anything.
///
/// Comparing the `covered` of the two criteria for the same budget gives
/// what the fairer allocation costs (Bertsimas, Farias and Trichakis,
/// 2011).
///
/// Errors ([`RouteError::Invalid`]): a gain that is negative, not finite
/// or larger than the one before it in its group; a `covered` that is
/// negative or not finite; a `total` that is negative or infinite; and
/// [`BudgetCriterion::Maximin`] with a total that is NaN.
pub fn allocate_budget(
    groups: &[BudgetGroup],
    budget: usize,
    criterion: BudgetCriterion,
) -> Result<BudgetAllocation, RouteError> {
    allocate_budget_with(groups, budget, criterion, &BudgetRules::default())
}

/// [`allocate_budget`] with weights, priority tiers and groups left out
/// ([`BudgetRules`]).
///
/// With weights, [`BudgetCriterion::Utilitarian`] gives each facility to
/// the group where the next one adds most once multiplied by the weight
/// of the group: the weighted gains of a group still do not increase, so
/// this is the largest weighted sum for every budget.
/// [`BudgetCriterion::Maximin`] gives it to the group with the lowest
/// share covered divided by its weight. With tiers the criterion only
/// compares the groups of the lowest tier that still has a facility to
/// open that adds something. A group that is not eligible gets none.
///
/// The gains, the demand covered and the lowest share of the result are
/// never weighted and take in every group, eligible or not, so that runs
/// with and without the rules can be compared.
///
/// Errors ([`RouteError::Invalid`]): those of [`allocate_budget`]; a list
/// of [`BudgetRules`] whose length is not the number of groups; and a
/// weight that is not finite or not above zero.
pub fn allocate_budget_with(
    groups: &[BudgetGroup],
    budget: usize,
    criterion: BudgetCriterion,
    rules: &BudgetRules,
) -> Result<BudgetAllocation, RouteError> {
    let invalid = |m: String| Err(RouteError::Invalid(m));
    for (what, len) in [
        ("weights", rules.weights.map(<[f64]>::len)),
        ("tiers", rules.tiers.map(<[u32]>::len)),
        ("eligible", rules.eligible.map(<[bool]>::len)),
    ] {
        if len.is_some_and(|n| n != groups.len()) {
            return invalid(format!(
                "the {what} of a budget allocation must be one per group: got {}, the groups are {}",
                len.unwrap_or(0),
                groups.len()
            ));
        }
    }
    if let Some((g, w)) = rules.weights.and_then(|w| {
        w.iter()
            .enumerate()
            .find(|(_, &w)| !(w.is_finite() && w > 0.0))
    }) {
        return invalid(format!(
            "the weight of a group must be finite and above zero, got {w} for group {g}"
        ));
    }
    let weight = |g: usize| rules.weights.map_or(1.0, |w| w[g]);
    let tier = |g: usize| rules.tiers.map_or(0, |t| t[g]);
    let eligible = |g: usize| rules.eligible.map_or(true, |e| e[g]);
    for (g, group) in groups.iter().enumerate() {
        for (k, &x) in group.marginal.iter().enumerate() {
            if !(x.is_finite() && x >= 0.0) {
                return invalid(format!(
                    "the gains must be finite and not negative, got {x} for facility {} of group {g}",
                    k + 1
                ));
            }
            if k > 0 && x > group.marginal[k - 1] * (1.0 + 1e-9) {
                return invalid(format!(
                    "the gains of a group may not increase: facility {} of group {g} adds {x}, the one before it {} (curves of max_cover solved without swaps do not increase)",
                    k + 1,
                    group.marginal[k - 1]
                ));
            }
        }
        if !(group.covered.is_finite() && group.covered >= 0.0) {
            return invalid(format!(
                "the demand covered in a group must be finite and not negative, got {} for group {g}",
                group.covered
            ));
        }
        if group.total < 0.0 || group.total.is_infinite() {
            return invalid(format!(
                "the demand of a group must be finite and not negative, got {} for group {g}",
                group.total
            ));
        }
        if criterion == BudgetCriterion::Maximin && group.total.is_nan() {
            return invalid(format!(
                "the maximin criterion needs the demand of every group, missing for group {g}"
            ));
        }
    }
    let mut covered: Vec<f64> = groups.iter().map(|g| g.covered).collect();
    let share = |g: usize, covered: &[f64]| -> f64 {
        let total = groups[g].total;
        if total > 0.0 {
            covered[g] / total
        } else if total == 0.0 {
            1.0
        } else {
            f64::NAN
        }
    };
    let mut out = BudgetAllocation {
        units: vec![0; groups.len()],
        priority: vec![None; groups.len()],
        ..BudgetAllocation::default()
    };
    let mut entered = 0usize;
    let mut sum = covered.iter().fold(0.0, |a, b| a + b);
    while out.group.len() < budget {
        // the groups with a facility left that adds something; `worth` is
        // the gain times the weight of the group
        let mut best: Option<(usize, f64, f64)> = None;
        for (g, group) in groups.iter().enumerate() {
            if !eligible(g) {
                continue;
            }
            let Some(&gain) = group.marginal.get(out.units[g]) else {
                continue;
            };
            if gain <= 0.0 {
                continue;
            }
            let worth = gain * weight(g);
            let wins = match best {
                None => true,
                Some((b, _, _)) if tier(g) != tier(b) => tier(g) < tier(b),
                Some((b, _, b_worth)) => match criterion {
                    BudgetCriterion::Utilitarian => worth > b_worth,
                    _ => {
                        let mine = share(g, &covered) / weight(g);
                        let theirs = share(b, &covered) / weight(b);
                        mine < theirs || (mine == theirs && worth > b_worth)
                    }
                },
            };
            if wins {
                best = Some((g, gain, worth));
            }
        }
        let Some((g, gain, _)) = best else { break };
        if out.units[g] == 0 {
            out.priority[g] = Some(entered);
            entered += 1;
        }
        out.units[g] += 1;
        covered[g] += gain;
        sum += gain;
        out.group.push(g);
        out.gain.push(gain);
        out.covered.push(sum);
        out.lowest_share.push(
            (0..groups.len())
                .map(|g| share(g, &covered))
                .fold(f64::INFINITY, f64::min),
        );
    }
    if groups.is_empty() || groups.iter().any(|g| g.total.is_nan()) {
        out.lowest_share.iter_mut().for_each(|x| *x = f64::NAN);
    }
    Ok(out)
}
