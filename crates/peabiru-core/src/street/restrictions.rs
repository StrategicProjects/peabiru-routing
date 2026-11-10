//! Turn restrictions from OSM `type=restriction` relations (R5
//! `StreetLayer.applyTurnRestriction`). Restrictions apply to cars only.

use super::StreetNetwork;
use crate::osm::pbf::{tag, Extract, MemberKind, Way};
use std::collections::HashMap;

/// A turn restriction over directed edges: from `from_edge`, through
/// `via_edges` in order, onto `to_edge`. A `no_` restriction forbids reaching
/// `to_edge` that way; an `only_` one forbids everything else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRestriction {
    /// OSM relation id.
    pub relation: i64,
    pub only: bool,
    pub from_edge: u32,
    pub via_edges: Vec<u32>,
    pub to_edge: u32,
}

/// Most partial paths the via-way search keeps. R5 enumerates every
/// ordering of the via ways that chains from the from-way, which grows
/// factorially when via ways share nodes; a relation that needs more paths
/// than this is dropped, as R5 drops one it cannot resolve. Real
/// restrictions (one to a few via ways in a chain) need a handful.
const MAX_VIA_PATHS: usize = 1_000;

/// Build the network's restrictions. `vertex_of` maps an OSM node id to the
/// last vertex created for it (R5 `vertexIndexForOsmNode`).
pub(crate) fn apply_turn_restrictions(
    net: &mut StreetNetwork,
    ex: &Extract,
    vertex_of: &dyn Fn(i64) -> Option<u32>,
) {
    let ways: HashMap<i64, &Way> = ex.ways.iter().map(|w| (w.id, w)).collect();
    let mut relations: Vec<_> = ex.relations.iter().collect();
    relations.sort_by_key(|r| r.id);
    for rel in relations {
        if let Some(r) = restriction_from_relation(net, ex, &ways, vertex_of, rel) {
            net.restrictions.push(r);
        }
    }
    net.restriction_starts = net
        .restrictions
        .iter()
        .enumerate()
        .map(|(i, r)| (r.from_edge, i as u32))
        .collect();
    net.restriction_starts.sort_unstable();
    for &(e, _) in &net.restriction_starts {
        net.edge_flags[e as usize] |= super::flags::RESTRICTION_START;
    }
}

/// The unique edge among `edges` whose OSM way is `way`; `Err` if there are
/// two (R5 treats that as ambiguous), `Ok(None)` if there is none.
fn unique_edge_of_way(net: &StreetNetwork, edges: &[u32], way: i64) -> Result<Option<u32>, ()> {
    let mut found = None;
    for &e in edges {
        if net.pair_way[e as usize / 2] == way {
            if found.is_some() {
                return Err(());
            }
            found = Some(e);
        }
    }
    Ok(found)
}

fn restriction_from_relation(
    net: &StreetNetwork,
    ex: &Extract,
    ways: &HashMap<i64, &Way>,
    vertex_of: &dyn Fn(i64) -> Option<u32>,
    rel: &crate::osm::pbf::Relation,
) -> Option<TurnRestriction> {
    let kind = rel.tags.get("restriction")?;
    let only = if kind.starts_with("no_") {
        false
    } else if kind.starts_with("only_") {
        true
    } else {
        return None;
    };
    let mut from = None;
    let mut to = None;
    let mut via = Vec::new();
    for m in &rel.members {
        match m.role.as_str() {
            "from" => {
                if from.is_some() || m.kind != MemberKind::Way {
                    return None;
                }
                from = Some(m);
            }
            "to" => {
                if to.is_some() || m.kind != MemberKind::Way {
                    return None;
                }
                to = Some(m);
            }
            "via" => via.push(m),
            _ => {}
        }
        // every way and node member must exist in the file
        let present = match m.kind {
            MemberKind::Way => ex.member_ways_present.binary_search(&m.id).is_ok(),
            MemberKind::Node => ex.member_nodes_present.binary_search(&m.id).is_ok(),
            MemberKind::Relation => true,
        };
        if !present {
            return None;
        }
    }
    let (from, to) = (from?, to?);
    if via.is_empty() {
        return None;
    }
    let via_ways = via.iter().any(|m| m.kind == MemberKind::Way);
    let via_nodes = via.iter().any(|m| m.kind == MemberKind::Node);
    if via.iter().any(|m| m.kind == MemberKind::Relation)
        || (via_ways && via_nodes)
        || (via_nodes && via.len() > 1)
    {
        return None;
    }

    if via_nodes {
        let v = vertex_of(via[0].id)? as usize;
        let from_edge = unique_edge_of_way(net, net.in_edges(v), from.id).ok()?;
        let to_edge = unique_edge_of_way(net, net.out_edges(v), to.id).ok()?;
        return Some(TurnRestriction {
            relation: rel.id,
            only,
            from_edge: from_edge?,
            via_edges: Vec::new(),
            to_edge: to_edge?,
        });
    }

    // Via ways: find the unique node path from the from-way through the via
    // ways to the to-way. Ways that are not routable have no edges, so the
    // restriction is dropped as R5 eventually does. The search gives up past
    // MAX_VIA_PATHS partial paths.
    let from_way = ways.get(&from.id)?;
    let to_way = ways.get(&to.id)?;
    let via_ids: Vec<i64> = via.iter().map(|m| m.id).collect();
    let via_way_nodes: Vec<&[i64]> = via_ids
        .iter()
        .map(|id| ways.get(id).map(|w| w.nodes.as_slice()))
        .collect::<Option<_>>()?;

    let mut nodes: Vec<Vec<i64>> = Vec::new();
    let mut paths: Vec<Vec<i64>> = Vec::new();
    for &node in &from_way.nodes {
        for (pos, vn) in via_way_nodes.iter().enumerate() {
            for &via_node in *vn {
                if node == via_node {
                    if nodes.len() == MAX_VIA_PATHS {
                        return None;
                    }
                    nodes.push(vec![node]);
                    paths.push(vec![via_ids[pos]]);
                }
            }
        }
    }
    for round in 0..via.len() - 1 {
        let (prev_nodes, prev_paths) = (std::mem::take(&mut nodes), std::mem::take(&mut paths));
        for (pn, pw) in prev_nodes.iter().zip(&prev_paths) {
            let way = ways.get(&pw[round])?;
            for &node in &way.nodes {
                for (pos, &via_id) in via_ids.iter().enumerate() {
                    if pw.contains(&via_id) {
                        continue;
                    }
                    for &via_node in via_way_nodes[pos] {
                        if via_node == node {
                            if nodes.len() == MAX_VIA_PATHS {
                                return None;
                            }
                            let mut nn = pn.clone();
                            nn.push(node);
                            let mut nw = pw.clone();
                            nw.push(via_id);
                            nodes.push(nn);
                            paths.push(nw);
                        }
                    }
                }
            }
        }
    }
    let mut path_nodes: Option<Vec<i64>> = None;
    let mut path_ways: &[i64] = &[];
    for (pn, pw) in nodes.iter().zip(&paths) {
        let final_way = ways.get(pw.last()?)?;
        for &node in &final_way.nodes {
            for &to_node in &to_way.nodes {
                if node == to_node {
                    if path_nodes.is_some() {
                        return None; // ambiguous
                    }
                    let mut p = pn.clone();
                    p.push(node);
                    path_nodes = Some(p);
                    path_ways = pw;
                }
            }
        }
    }
    let path_nodes = path_nodes?;

    let from_vertex = vertex_of(path_nodes[0])? as usize;
    let from_edge = unique_edge_of_way(net, net.in_edges(from_vertex), from.id).ok()?;
    let to_vertex = vertex_of(*path_nodes.last()?)? as usize;
    let to_edge = unique_edge_of_way(net, net.out_edges(to_vertex), to.id).ok()?;
    let (from_edge, to_edge) = (from_edge?, to_edge?);
    let mut via_edges = Vec::new();
    for nidx in (1..path_nodes.len()).rev() {
        let way_id = path_ways[nidx - 1];
        let v = vertex_of(path_nodes[nidx])? as usize;
        via_edges.push(unique_edge_of_way(net, net.in_edges(v), way_id).ok()??);
    }
    via_edges.reverse();
    Some(TurnRestriction {
        relation: rel.id,
        only,
        from_edge,
        via_edges,
        to_edge,
    })
}

/// Relations worth reading for the street network.
pub(crate) fn is_restriction(t: &[(&str, &str)]) -> bool {
    tag(t, "type") == Some("restriction")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osm::pbf::{Member, Relation, Tags};
    use crate::street::build::network_from_extract;
    use crate::street::router::{street_seconds, Mode};

    const N: usize = 7;
    const C: usize = 3; // central row and column

    fn node(r: usize, c: usize) -> i64 {
        (1 + r * N + c) as i64
    }

    /// A 7 x 7 grid of two-way residential streets, nodes 1e-3 degrees apart
    /// near the equator. Row `r` is way `1000 + r` and column `c` way
    /// `2000 + c`, except the central row and column, which are split at the
    /// centre as OSM usually does around a restricted junction: row 3 is
    /// `1003` (west half) and `1103` (east half), column 3 is `2003` (rows
    /// 0-3) and `2103` (rows 3-6).
    fn grid(relations: Vec<Relation>) -> Extract {
        let tags = || Tags(vec![("highway".into(), "residential".into())]);
        let way = |id: i64, nodes: Vec<i64>| Way {
            id,
            tags: tags(),
            nodes,
        };
        let mut ways = Vec::new();
        for r in 0..N {
            let row: Vec<i64> = (0..N).map(|c| node(r, c)).collect();
            if r == C {
                ways.push(way(1003, row[..=C].to_vec()));
                ways.push(way(1103, row[C..].to_vec()));
            } else {
                ways.push(way(1000 + r as i64, row));
            }
        }
        for c in 0..N {
            let col: Vec<i64> = (0..N).map(|r| node(r, c)).collect();
            if c == C {
                ways.push(way(2003, col[..=C].to_vec()));
                ways.push(way(2103, col[C..].to_vec()));
            } else {
                ways.push(way(2000 + c as i64, col));
            }
        }
        let node_ids: Vec<i64> = (0..N * N).map(|i| 1 + i as i64).collect();
        let lat = node_ids
            .iter()
            .map(|&i| (((i - 1) as usize / N) * 10_000) as i32)
            .collect();
        let lon = node_ids
            .iter()
            .map(|&i| (((i - 1) as usize % N) * 10_000) as i32)
            .collect();
        let mut member_ways_present: Vec<i64> = ways.iter().map(|w| w.id).collect();
        member_ways_present.sort_unstable();
        Extract {
            ref_counts: vec![2; node_ids.len()], // every node is an intersection
            member_nodes_present: node_ids.clone(),
            member_ways_present,
            node_ids,
            lat,
            lon,
            ways,
            relations,
            node_tags: Vec::new(),
            pois: Vec::new(),
        }
    }

    fn restriction(kind: &str, from: i64, via_node: i64, to: i64) -> Relation {
        Relation {
            id: 1,
            tags: Tags(vec![
                ("type".into(), "restriction".into()),
                ("restriction".into(), kind.into()),
            ]),
            members: vec![
                Member {
                    kind: MemberKind::Way,
                    id: from,
                    role: "from".into(),
                },
                Member {
                    kind: MemberKind::Node,
                    id: via_node,
                    role: "via".into(),
                },
                Member {
                    kind: MemberKind::Way,
                    id: to,
                    role: "to".into(),
                },
            ],
        }
    }

    /// Car seconds from a point on row 3 west of the centre to a point on
    /// column 3 south of it (lower latitude): the direct route turns at the
    /// centre from 1003 onto 2003.
    fn car_seconds(relations: Vec<Relation>) -> i64 {
        let net = network_from_extract(grid(relations));
        let on_row = (C as f64 * 1e-3 + 2e-5, 1.0 * 1e-3 + 3e-4);
        let on_col = (1.0 * 1e-3 + 3e-4, C as f64 * 1e-3 + 2e-5);
        street_seconds(&net, on_row, on_col, Mode::Car).expect("reachable")
    }

    #[test]
    fn via_node_restrictions_are_built() {
        let net = network_from_extract(grid(vec![restriction(
            "no_right_turn",
            1003,
            node(C, C),
            2003,
        )]));
        assert_eq!(net.restrictions.len(), 1);
        let r = &net.restrictions[0];
        assert!(!r.only && r.via_edges.is_empty());
        assert_eq!(net.pair_way[r.from_edge as usize / 2], 1003);
        assert_eq!(net.pair_way[r.to_edge as usize / 2], 2003);
        assert_eq!(
            net.edge_to(r.from_edge as usize),
            net.edge_from(r.to_edge as usize)
        );
        assert!(
            net.edge_flags[r.from_edge as usize] & crate::street::flags::RESTRICTION_START != 0
        );
    }

    #[test]
    fn ambiguous_or_incomplete_relations_are_dropped() {
        // way 1002 runs through node (2, 3): two incoming edges, ambiguous
        let net = network_from_extract(grid(vec![restriction(
            "no_left_turn",
            1002,
            node(2, C),
            2003,
        )]));
        assert!(net.restrictions.is_empty());
        // not a no_/only_ restriction
        let net = network_from_extract(grid(vec![restriction("give_way", 1003, node(C, C), 2003)]));
        assert!(net.restrictions.is_empty());
        // a member missing from the file
        let mut ex = grid(vec![restriction("no_right_turn", 1003, node(C, C), 2003)]);
        ex.member_nodes_present.retain(|&i| i != node(C, C));
        assert!(network_from_extract(ex).restrictions.is_empty());
    }

    fn via_way_restriction(id: i64, from: i64, via: &[i64], to: i64) -> Relation {
        let way = |id: i64, role: &str| Member {
            kind: MemberKind::Way,
            id,
            role: role.into(),
        };
        let mut members = vec![way(from, "from")];
        members.extend(via.iter().map(|&v| way(v, "via")));
        members.push(way(to, "to"));
        Relation {
            id,
            tags: Tags(vec![
                ("type".into(), "restriction".into()),
                ("restriction".into(), "no_u_turn".into()),
            ]),
            members,
        }
    }

    #[test]
    fn via_way_restrictions_are_built() {
        let net = network_from_extract(grid(vec![
            via_way_restriction(1, 1003, &[2103], 1103),
            // both orders of the two via ways reach the to-way: ambiguous
            via_way_restriction(2, 1003, &[2103, 2003], 1103),
        ]));
        assert_eq!(net.restrictions.len(), 1);
        let r = &net.restrictions[0];
        let ways: Vec<i64> = std::iter::once(r.from_edge)
            .chain(r.via_edges.iter().copied())
            .chain([r.to_edge])
            .map(|e| net.pair_way[e as usize / 2])
            .collect();
        assert_eq!((r.relation, ways), (1, vec![1003, 2103, 1103]));
    }

    /// Via ways that all share the same two nodes chain in every order: the
    /// search stops at its bound and the relation is dropped (R5 finds it
    /// ambiguous, after enumerating 12! orderings).
    #[test]
    fn pathological_via_ways_are_dropped_quickly() {
        let k = 12;
        let via: Vec<i64> = (0..k).map(|i| 5000 + i).collect();
        let mut ex = grid(vec![via_way_restriction(1, 1003, &via, 2000)]);
        for &id in &via {
            ex.ways.push(Way {
                id,
                tags: Tags(vec![("highway".into(), "residential".into())]),
                nodes: vec![node(C, 0), node(C, 1)],
            });
            ex.member_ways_present.push(id);
        }
        ex.member_ways_present.sort_unstable();
        let t = std::time::Instant::now();
        let net = network_from_extract(ex);
        assert!(net.restrictions.is_empty());
        assert!(t.elapsed().as_secs() < 5, "{:?}", t.elapsed());
    }

    #[test]
    fn restrictions_force_a_detour_only_where_they_apply() {
        let free = car_seconds(vec![]);
        // forbidding the turn the direct route takes costs a detour
        let no_turn = car_seconds(vec![restriction("no_right_turn", 1003, node(C, C), 2003)]);
        assert!(no_turn > free, "{no_turn} > {free}");
        // "only straight on" forbids it too
        let only = car_seconds(vec![restriction(
            "only_straight_on",
            1003,
            node(C, C),
            1103,
        )]);
        assert!(only > free, "{only} > {free}");
        // a restriction on another turn changes nothing
        let other = car_seconds(vec![restriction("no_left_turn", 1003, node(C, C), 2103)]);
        assert_eq!(other, free);
        // an "only" restriction that the direct route obeys changes nothing
        let obeyed = car_seconds(vec![restriction("only_right_turn", 1003, node(C, C), 2003)]);
        assert_eq!(obeyed, free);
    }
}
