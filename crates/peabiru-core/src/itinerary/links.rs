//! r5r's `osm_link_ids = TRUE` columns: the OpenStreetMap ways and the
//! street edges of each street leg, and the stops of each ride.
//!
//! A street leg's ids come from R5's `StreetSegment.streetEdges` (direct
//! trips, access legs and walks between stops) or from r5r's
//! `EgressPath.build` (the leg from the last stop): one entry per edge
//! traversed, the egress leg's partial edge included when it has a length.
//! `StreetSegment` merges entries first (`compactEdges`): an entry joins the
//! previous group when both are "similar", and the group keeps its first
//! edge. Similar means the same mode, and either both "continue" on a street
//! of the same name, or both turn on a roundabout. r5r's network has no
//! street names (R5 drops its OSM database once the network is built, and
//! `StreetLayer.getNameEdgeIdx` then returns null for every edge), so only
//! the roundabout rule applies: an edge of a `junction=roundabout` way,
//! after the first edge of the leg, joins a group that starts on such an
//! edge with the same mode. r5r then keeps, in order and once each, the
//! OSM way id of each group's edge and that edge's index, when the way id
//! is positive (connectors have none).

use crate::street::flags::ROUNDABOUT;
use crate::street::p2p::Router;
use crate::street::{Mode, StreetNetwork};

/// The ids of one leg (empty lists and stop ids for most legs).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Links {
    pub(crate) osm: Vec<i64>,
    pub(crate) edges: Vec<u32>,
    pub(crate) board_stop: String,
    pub(crate) alight_stop: String,
}

/// The edges of the path ending at state `last`, with the mode each was
/// traversed by (`StreetPath.getStates()`, states with a back edge).
pub(crate) fn path_edges(r: &Router, last: u32) -> Vec<(u32, Mode)> {
    r.path(last)
        .into_iter()
        .filter_map(|id| {
            let s = &r.states[id as usize];
            (s.back_edge >= 0).then_some((s.back_edge as u32, s.mode))
        })
        .collect()
}

/// `StreetSegment` (`compactEdges`) then `TripLeg`'s lists.
pub(crate) fn compacted(net: &StreetNetwork, edges: &[(u32, Mode)]) -> Links {
    let mut heads = Vec::with_capacity(edges.len());
    // the group's first edge, its mode and whether it turns on a roundabout
    let mut head: Option<(u32, Mode, bool)> = None;
    for (i, &(e, mode)) in edges.iter().enumerate() {
        let circle = i > 0 && net.edge_flags[e as usize] & ROUNDABOUT != 0;
        if let Some((h, hm, hc)) = head {
            if hc && circle && hm == mode {
                continue;
            }
            heads.push(h);
        }
        head = Some((e, mode, circle));
    }
    if let Some((h, _, _)) = head {
        heads.push(h);
    }
    lists(net, heads)
}

/// `TripLeg`'s lists from every edge given, in order.
pub(crate) fn lists(net: &StreetNetwork, edges: impl IntoIterator<Item = u32>) -> Links {
    let mut l = Links::default();
    for e in edges {
        let osm = net.pair_way[e as usize / 2];
        if osm > 0 {
            // LinkedHashSet.add: the first time only
            if !l.osm.contains(&osm) {
                l.osm.push(osm);
            }
            if !l.edges.contains(&e) {
                l.edges.push(e);
            }
        }
    }
    l
}

/// `LinkedHashSet.toString()`: `[a, b, c]`, `[]` when empty.
pub(crate) fn list_text<T: std::fmt::Display>(v: &[T]) -> String {
    let mut s = String::from("[");
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&x.to_string());
    }
    s.push(']');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_print_as_java_sets() {
        assert_eq!(list_text::<i64>(&[]), "[]");
        assert_eq!(list_text(&[5i64]), "[5]");
        assert_eq!(list_text(&[5i64, -2, 7]), "[5, -2, 7]");
    }
}
