//! Itineraries whose result could depend on the order R5 has left the via
//! edges of turn restrictions in.
//!
//! R5's `canTurnFrom` reverses a restriction's array of via edges in place
//! (`TurnRestriction.reverse`) every time a reverse car search checks a
//! restriction with two or more of them. The array belongs to the network
//! that every search shares, so the order a check sees, in a reverse search
//! or a forward one, depends on how many checks of that restriction came
//! before it: in the same search, in the other pairs of the call (which r5r
//! routes in parallel, in an order that is not fixed) and in earlier calls
//! of the session. In r5r's `detailed_itineraries()` this happens with
//! `mode_egress = "CAR"`, whose egress search runs backwards; the car
//! searches of such a call (egress, and car access and direct trips) may
//! then see either order.
//!
//! A check is "ambiguous" when its outcome changes with the order (the edge
//! taken is the via edge expected in one order and not in the other). A pair
//! whose searches meet no ambiguous check gets R5's result whatever the
//! history. A pair with some is planned again for every combination of
//! orders its ambiguous checks can see (each one on its own, as threads can
//! interleave anywhere); when the itineraries are the same in all of them,
//! they are R5's whatever the history. Otherwise the pair is an error.

use crate::street::p2p::Router;
use crate::RouteError;

/// Most plans of one pair before giving up.
const MAX_RUNS: usize = 256;

/// The orders the ambiguous checks of one plan take, shared by its car
/// searches in the order they run.
#[derive(Default)]
pub(crate) struct ViaOrders {
    /// `None`: the network as built (no reverse car search in the call).
    choices: Option<Vec<bool>>,
    used: Vec<bool>,
}

impl ViaOrders {
    /// Give `r` the orders of the checks still to come.
    pub(crate) fn prepare(&self, r: &mut Router) {
        r.via_choices = self
            .choices
            .as_ref()
            .map(|c| c.get(self.used.len()..).unwrap_or(&[]).to_vec());
    }

    /// Record the orders `r`'s search took.
    pub(crate) fn collect(&mut self, r: &mut Router) {
        self.used.append(&mut r.via_used);
    }
}

/// Run `plan` once with the network as built when `free` is false;
/// otherwise for every order the ambiguous checks can see. Returns the
/// first result, or an error when the results differ (or there are too
/// many orders to try).
pub(crate) fn settle<R: PartialEq>(
    free: bool,
    pair: usize,
    mut plan: impl FnMut(&mut ViaOrders) -> Result<R, RouteError>,
) -> Result<R, RouteError> {
    if !free {
        return plan(&mut ViaOrders::default());
    }
    let mut run = |choices: Vec<bool>| {
        let mut v = ViaOrders {
            choices: Some(choices),
            used: Vec::new(),
        };
        plan(&mut v).map(|r| (r, v.used))
    };
    let (first, used) = run(Vec::new())?;
    let mut todo = children(0, &used);
    let mut runs = 1;
    while let Some(prefix) = todo.pop() {
        runs += 1;
        if runs > MAX_RUNS {
            return Err(depends(pair, "there are too many orders to check"));
        }
        let n = prefix.len();
        let (r, used) = run(prefix)?;
        if r != first {
            return Err(depends(pair, "its itineraries change with that order"));
        }
        todo.extend(children(n, &used));
    }
    Ok(first)
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

fn depends(pair: usize, why: &str) -> RouteError {
    RouteError::Invalid(format!(
        "not supported: a car search of pair {} crosses a turn restriction with two or more \
         via ways, whose via edges R5 reverses in place on every check of its car egress \
         search, so r5r's itineraries depend on the searches that ran before ({why})",
        pair + 1
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake plan: `n` ambiguous checks; the result counts the reversed
    /// orders when `count`, else is constant.
    fn fake(n: usize, count: bool) -> impl FnMut(&mut ViaOrders) -> Result<usize, RouteError> {
        move |v| {
            let ch = v.choices.clone().unwrap_or_default();
            v.used = (0..n).map(|i| ch.get(i).copied().unwrap_or(true)).collect();
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
        let mut f = fake(5, false);
        let r = settle(true, 0, |v| {
            let r = f(v);
            seen.insert(v.used.clone());
            r
        });
        assert_eq!(r.ok(), Some(7));
        assert_eq!(seen.len(), 32);
        assert!(settle(true, 0, fake(3, true)).is_err());
        assert!(settle(true, 0, fake(9, false)).is_err()); // 512 > MAX_RUNS
        assert_eq!(settle(false, 0, fake(3, true)).ok(), Some(3));
    }
}
