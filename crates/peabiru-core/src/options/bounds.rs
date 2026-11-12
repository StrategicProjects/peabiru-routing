//! Lower bounds, per stop, of what is still to come on the way to the
//! destination of one pair: the least time, rides and time on foot from
//! being at a stop (off a vehicle, or about to board) to the destination,
//! each over the trips the search allows, computed backwards from the egress
//! stops with Dijkstra's algorithm on a graph of stops and vehicle
//! positions. The search uses them as A* uses its heuristic: to drop a
//! label that cannot reach the destination within the limits, or whose
//! best possible values a trip already found dominates.
//!
//! The ride times are the least over the running vehicles of each hop
//! between consecutive stops and of each stop's dwell, which bound every
//! vehicle's times from below only when they never decrease along a trip;
//! on a network with a trip whose times do, there are no bounds (every stop
//! gets 0).

use super::search::{Network, StopLeg};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Unreachable.
pub(crate) const INF: i32 = i32::MAX;

/// Per stop: `[time, rides, walk]` bounds off a vehicle and about to board.
pub(crate) struct Bounds {
    pub(crate) off: Vec<[i32; 3]>,
    pub(crate) on: Vec<[i32; 3]>,
}

/// Seconds of the boarding wait the bounds count (the search waits at
/// least this long).
const BOARD: i32 = 60;

impl Bounds {
    /// No bounds: everything reachable at no cost.
    pub(crate) fn none(n_stops: usize) -> Self {
        Bounds {
            off: vec![[0; 3]; n_stops],
            on: vec![[0; 3]; n_stops],
        }
    }

    /// The bounds of a pair whose egress legs are `egress`.
    pub(crate) fn new(net: &Network, egress: &[StopLeg]) -> Self {
        let n = net.layer.stop_ids.len();
        let Some(g) = &net.graph else {
            return Bounds::none(n);
        };
        let mut off = vec![[INF; 3]; n];
        let mut on = vec![[INF; 3]; n];
        for k in 0..3 {
            let (o, b) = dijkstra(net, g, egress, k);
            for s in 0..n {
                off[s][k] = o[s];
                on[s][k] = b[s];
            }
        }
        Bounds { off, on }
    }
}

/// The graph behind the bounds, built once per call.
pub(crate) struct Graph {
    /// Per stop, the `(pattern, position)` of the running patterns there.
    pos_of_stop: Vec<Vec<(u32, u32)>>,
    /// Node index of a pattern's first position (`base[p] + j`, after the
    /// two nodes of every stop); `u32::MAX` for a pattern not running.
    base: Vec<u32>,
    /// Per running pattern and position `j`: the least `arrival[j+1] -
    /// departure[j]` and `departure[j] - arrival[j]` over its vehicles.
    hop: Vec<Vec<i32>>,
    dwell: Vec<Vec<i32>>,
    /// Per stop, the walks into it: `(from stop, seconds)`.
    rev_transfers: Vec<Vec<(u32, i32)>>,
    /// The pattern of each vehicle-position node (from node `2 * n`).
    owner: Vec<u32>,
    n_nodes: usize,
}

impl Graph {
    /// `None` when a running vehicle's times decrease along its trip.
    pub(crate) fn new(net: &Network) -> Option<Graph> {
        let layer = net.layer;
        let n = layer.stop_ids.len();
        let mut pos_of_stop = vec![Vec::new(); n];
        let mut base = vec![u32::MAX; layer.patterns.len()];
        let mut hop = vec![Vec::new(); layer.patterns.len()];
        let mut dwell = vec![Vec::new(); layer.patterns.len()];
        let mut next = 2 * n;
        let mut owner = Vec::new();
        for (pi, run) in net.runs.iter().enumerate() {
            let Some(run) = run else { continue };
            let pat = &layer.patterns[pi];
            let len = pat.stops.len();
            let mut h = vec![i32::MAX; len];
            let mut w = vec![i32::MAX; len];
            for &t in run.sched.iter().chain(&run.freq) {
                let trip = &pat.trips[t as usize];
                for j in 0..len {
                    let dw = trip.departures[j].checked_sub(trip.arrivals[j])?;
                    if dw < 0 {
                        return None;
                    }
                    w[j] = w[j].min(dw);
                    if j + 1 < len {
                        let hp = trip.arrivals[j + 1].checked_sub(trip.departures[j])?;
                        if hp < 0 {
                            return None;
                        }
                        h[j] = h[j].min(hp);
                    }
                }
            }
            base[pi] = next as u32;
            next += len;
            owner.extend(std::iter::repeat(pi as u32).take(len));
            for (j, &s) in pat.stops.iter().enumerate() {
                pos_of_stop[s as usize].push((pi as u32, j as u32));
            }
            hop[pi] = h;
            dwell[pi] = w;
        }
        let mut rev_transfers = vec![Vec::new(); n];
        for (s, list) in net.transfers.iter().enumerate() {
            for &(t, mm) in list {
                if mm < net.cfg.max_transfer_mm {
                    rev_transfers[t as usize].push((s as u32, mm / net.cfg.walk_mms));
                }
            }
        }
        Some(Graph {
            pos_of_stop,
            base,
            hop,
            dwell,
            rev_transfers,
            owner,
            n_nodes: next,
        })
    }
}

/// Backward Dijkstra for criterion `k` (0 time, 1 rides, 2 walk): the
/// bounds off a vehicle and about to board, per stop.
fn dijkstra(net: &Network, g: &Graph, egress: &[StopLeg], k: usize) -> (Vec<i32>, Vec<i32>) {
    let layer = net.layer;
    let n = layer.stop_ids.len();
    let mut dist = vec![INF; g.n_nodes];
    let mut heap: BinaryHeap<Reverse<(i64, u32)>> = BinaryHeap::new();
    let push = |dist: &mut Vec<i32>, heap: &mut BinaryHeap<_>, node: usize, d: i64| {
        let d = d.min(i64::from(INF - 1));
        if d < i64::from(dist[node]) {
            dist[node] = d as i32;
            heap.push(Reverse((d, node as u32)));
        }
    };
    for e in egress {
        let w = match k {
            0 => e.secs,
            1 => 0,
            _ => e.walk,
        };
        push(&mut dist, &mut heap, e.stop as usize, i64::from(w));
    }
    while let Some(Reverse((d, node))) = heap.pop() {
        let node = node as usize;
        if d > i64::from(dist[node]) {
            continue;
        }
        if node < n {
            // off a vehicle at stop `node`: reached by alighting there
            for &(p, j) in &g.pos_of_stop[node] {
                if layer.patterns[p as usize].dropoffs[j as usize] {
                    let v = g.base[p as usize] as usize + j as usize;
                    push(&mut dist, &mut heap, v, d);
                }
            }
        } else if node < 2 * n {
            // about to board at stop `s`: reached off a vehicle there, or by
            // walking from another stop
            let s = node - n;
            push(&mut dist, &mut heap, s, d);
            for &(from, secs) in &g.rev_transfers[s] {
                let w = if k == 1 { 0 } else { i64::from(secs) };
                push(&mut dist, &mut heap, from as usize, d + w);
            }
        } else {
            // on a vehicle of pattern p having reached position j
            let p = g.owner[node - 2 * n] as usize;
            let j = node - g.base[p] as usize;
            if j == 0 {
                continue;
            }
            let pat = &layer.patterns[p];
            let (h, w) = (i64::from(g.hop[p][j - 1]), i64::from(g.dwell[p][j - 1]));
            let prev = node - 1;
            if k == 0 {
                push(&mut dist, &mut heap, prev, d + w + h);
            } else {
                push(&mut dist, &mut heap, prev, d);
            }
            if pat.pickups[j - 1] {
                let s = pat.stops[j - 1] as usize;
                let wt = match k {
                    0 => i64::from(BOARD) + h,
                    1 => 1,
                    _ => 0,
                };
                push(&mut dist, &mut heap, n + s, d + wt);
            }
        }
    }
    (dist[..n].to_vec(), dist[n..2 * n].to_vec())
}
