//! Itineraries whose result could depend on the order R5 has left the via
//! edges of turn restrictions in.
//!
//! R5's `canTurnFrom` reverses a restriction's array of via edges in place
//! (`TurnRestriction.reverse`) every time a reverse car search checks a
//! restriction with two or more of them. The array belongs to the network
//! that every search shares, so the order a check sees, in a reverse search
//! or a forward one, depends on how many checks of that restriction came
//! before it: in the same search, in the other pairs of the call (which R5's
//! callers route in parallel, in an order that is not fixed) and in earlier calls
//! of the session. In `detailed_itineraries()` this happens with
//! `mode_egress = "CAR"`, whose egress search runs backwards; the car
//! searches of such a call (egress, and car access and direct trips) may
//! then see either order.
//!
//! peabiru plans each pair as R5 does on a network on which no reverse car
//! search has run before it (a freshly built network, the pair alone, one
//! thread): every array as built, reversed by each check of the pair's own
//! reverse searches, in the order its searches run. To say when R5 could
//! give something else, a pair whose searches meet an "ambiguous" check
//! (one whose outcome changes with the order: the edge taken is the via
//! edge expected in one order and not in the other) is planned again for
//! every combination of orders its ambiguous checks can see (each one on its
//! own, as threads can interleave anywhere); the pair "depends on the
//! history" when some combination gives other itineraries.

use crate::street::p2p::Router;
use crate::RouteError;
use std::collections::HashMap;

/// Most plans of one pair when looking for other itineraries; a pair that
/// needs more counts as depending on the history.
const MAX_RUNS: usize = 256;

/// The orders the restriction checks of one plan take, shared by its car
/// searches in the order they run.
#[derive(Default)]
pub(crate) struct ViaOrders {
    /// `None` with `fresh` `None`: the network as built (no reverse car
    /// search in the call).
    choices: Option<Vec<bool>>,
    /// The orders of a network on which only this pair's searches ran
    /// (see `Router::via_fresh`).
    fresh: Option<HashMap<u64, bool>>,
    used: Vec<bool>,
    /// Set by the plan when the pair is to be planned again with McRAPTOR.
    pub(crate) mcraptor_fallback: bool,
}

impl ViaOrders {
    /// Every check sees the via edges in the order of its own search (as
    /// built in a forward search, reversed in a reverse one): the turn
    /// restrictions as OpenStreetMap states them, whatever ran before. For
    /// peabiru's own functions, which do not reproduce R5's in-place
    /// reversal.
    pub(super) fn as_stated() -> Self {
        ViaOrders {
            choices: Some(Vec::new()),
            ..ViaOrders::default()
        }
    }

    /// Give `r` the orders of the checks still to come.
    pub(crate) fn prepare(&self, r: &mut Router) {
        r.via_choices = self
            .choices
            .as_ref()
            .map(|c| c.get(self.used.len()..).unwrap_or(&[]).to_vec());
        r.via_fresh = self.fresh.clone();
    }

    /// Record the orders `r`'s search took.
    pub(crate) fn collect(&mut self, r: &mut Router) {
        self.used.append(&mut r.via_used);
        if let Some(f) = r.via_fresh.take() {
            self.fresh = Some(f);
        }
    }
}

/// Run `plan` once with the network as built when `free` is false.
/// Otherwise run it with the orders of a fresh network (see the module),
/// and then for every other order its ambiguous checks can see. Returns the
/// first result, and whether some order gave another one (or there were too
/// many orders to try).
pub(crate) fn settle<R: PartialEq>(
    free: bool,
    mut plan: impl FnMut(&mut ViaOrders) -> Result<R, RouteError>,
) -> Result<(R, bool), RouteError> {
    if !free {
        return Ok((plan(&mut ViaOrders::default())?, false));
    }
    let mut fresh = ViaOrders {
        fresh: Some(HashMap::new()),
        ..ViaOrders::default()
    };
    let first = plan(&mut fresh)?;
    let mut todo = children(0, &fresh.used);
    let mut runs = 1;
    while let Some(prefix) = todo.pop() {
        runs += 1;
        if runs > MAX_RUNS {
            return Ok((first, true));
        }
        let n = prefix.len();
        let mut v = ViaOrders {
            choices: Some(prefix),
            ..ViaOrders::default()
        };
        let r = plan(&mut v)?;
        if r != first {
            return Ok((first, true));
        }
        todo.extend(children(n, &v.used));
    }
    Ok((first, false))
}

/// The prefixes that take the other order at one of the checks after the
/// first `from`.
fn children(from: usize, used: &[bool]) -> Vec<Vec<bool>> {
    (from..used.len())
        .map(|i| {
            let mut c = used[..i].to_vec();
            c.push(!used[i]);
            c
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake plan: `n` ambiguous checks, which take `true` in a fresh plan
    /// when `fresh_true`; the result counts the reversed orders when
    /// `count`, else is constant.
    fn fake(
        n: usize,
        count: bool,
        fresh_true: bool,
    ) -> impl FnMut(&mut ViaOrders) -> Result<usize, RouteError> {
        move |v| {
            v.used = match (&v.fresh, &v.choices) {
                (Some(_), _) => vec![fresh_true; n],
                (None, Some(ch)) => (0..n).map(|i| ch.get(i).copied().unwrap_or(true)).collect(),
                (None, None) => vec![false; n],
            };
            Ok(if count {
                v.used.iter().filter(|&&b| b).count()
            } else {
                7
            })
        }
    }

    #[test]
    fn every_order_is_tried() {
        let mut seen = std::collections::HashSet::new();
        let mut f = fake(5, false, false);
        let r = settle(true, |v| {
            let r = f(v);
            seen.insert(v.used.clone());
            r
        });
        assert_eq!(r.ok(), Some((7, false)));
        assert_eq!(seen.len(), 32);
        // the fresh plan's result is returned, and the pair depends
        assert_eq!(settle(true, fake(3, true, false)).ok(), Some((0, true)));
        assert_eq!(settle(true, fake(3, true, true)).ok(), Some((3, true)));
        // 512 orders > MAX_RUNS: counted as depending
        assert_eq!(settle(true, fake(9, false, false)).ok(), Some((7, true)));
        assert_eq!(settle(false, fake(3, true, true)).ok(), Some((0, false)));
        assert_eq!(settle(true, fake(0, true, true)).ok(), Some((0, false)));
    }
}
