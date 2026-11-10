//! Contraction of a graph with integer arc times into a hierarchy.

use super::NONE;
use crate::CancelToken;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// A contraction hierarchy with nodes renumbered by rank, highest first
/// (`id[node]`), so a sweep from the top runs through memory in order.
pub(super) struct Hierarchy {
    pub(super) id: Vec<u32>,
    /// Upward arcs `(to, seconds)` of each node, to lower ids.
    pub(super) up_off: Vec<u32>,
    pub(super) up: Vec<(u32, u32)>,
    /// Downward arcs into each node `(from, seconds)`, from lower ids.
    pub(super) down_off: Vec<u32>,
    pub(super) down: Vec<(u32, u32)>,
    pub(super) shortcuts: usize,
    pub(super) rounds: usize,
}

/// Dijkstra scratch for witness searches.
struct Witness {
    dist: Vec<u32>,
    touched: Vec<u32>,
    heap: BinaryHeap<Reverse<(u32, u32)>>,
    /// Run stamp of the nodes a run looks for, so it stops once all are
    /// settled.
    target: Vec<u32>,
    stamp: u32,
    /// The shortcuts of the last [`Witness::shortcuts_of`].
    found: Vec<(u32, u32, u32)>,
    settle: usize,
}

/// Nodes a witness search may settle before it gives up (a shortcut is
/// then added; more shortcuts than needed never change a time).
const WITNESS_SETTLE: usize = 32;
/// The same when only estimating a node's priority: a rough count is
/// enough, and these searches are most of the contraction's work.
const ESTIMATE_SETTLE: usize = 2;
/// Fewer items than this are not worth spawning threads for.
const PAR_MIN_ITEMS: usize = 64;

impl Witness {
    fn new(n: usize) -> Self {
        Witness {
            dist: vec![NONE; n],
            touched: Vec::new(),
            heap: BinaryHeap::new(),
            target: vec![0; n],
            stamp: 0,
            found: Vec::new(),
            settle: WITNESS_SETTLE,
        }
    }

    /// Least time from `s` in the remaining graph without `skip` and the
    /// `blocked` nodes, as far as `bound` and the settle limit allow, until
    /// the `targets` marked with the current stamp are settled.
    fn run(
        &mut self,
        out: &[Vec<(u32, u32)>],
        blocked: &[bool],
        skip: u32,
        s: u32,
        bound: u32,
        mut targets: usize,
    ) {
        for &v in &self.touched {
            self.dist[v as usize] = NONE;
        }
        self.touched.clear();
        self.heap.clear();
        self.dist[s as usize] = 0;
        self.touched.push(s);
        self.heap.push(Reverse((0, s)));
        let mut settled = 0;
        while let Some(Reverse((d, u))) = self.heap.pop() {
            if d > self.dist[u as usize] {
                continue;
            }
            if d > bound || settled >= self.settle {
                break;
            }
            settled += 1;
            if self.target[u as usize] == self.stamp {
                targets -= 1;
                if targets == 0 {
                    break;
                }
            }
            for &(v, w) in &out[u as usize] {
                if v == skip || blocked[v as usize] {
                    continue;
                }
                let nd = d.saturating_add(w);
                if nd < self.dist[v as usize] {
                    if self.dist[v as usize] == NONE {
                        self.touched.push(v);
                    }
                    self.dist[v as usize] = nd;
                    self.heap.push(Reverse((nd, v)));
                }
            }
        }
    }

    /// Into `self.found`, the shortcuts `(u, v, seconds)` that contracting
    /// `x` needs: one per pair of neighbours `u -> x -> v` without a path
    /// of no more time that avoids `x` and the `blocked` nodes.
    fn shortcuts_of(
        &mut self,
        out: &[Vec<(u32, u32)>],
        inn: &[Vec<(u32, u32)>],
        blocked: &[bool],
        x: u32,
    ) {
        self.found.clear();
        let outs = &out[x as usize];
        for &(u, wu) in &inn[x as usize] {
            self.stamp = self.stamp.wrapping_add(1);
            if self.stamp == 0 {
                self.target.iter_mut().for_each(|t| *t = 0);
                self.stamp = 1;
            }
            let mut bound = 0u32;
            let mut targets = 0;
            for &(v, wv) in outs {
                if v != u {
                    bound = bound.max(wu.saturating_add(wv));
                    self.target[v as usize] = self.stamp;
                    targets += 1;
                }
            }
            if targets == 0 {
                continue;
            }
            self.run(out, blocked, x, u, bound, targets);
            for &(v, wv) in outs {
                let via = wu.saturating_add(wv);
                if v != u && self.dist[v as usize] > via {
                    self.found.push((u, v, via));
                }
            }
        }
    }
}

/// Add arc `u -> v` or lower its time.
fn add_arc(list: &mut Vec<(u32, u32)>, v: u32, w: u32) {
    match list.iter_mut().find(|a| a.0 == v) {
        Some(a) => a.1 = a.1.min(w),
        None => list.push((v, w)),
    }
}

/// `f(worker scratch, item)` over `items`, in contiguous chunks over the
/// workers when there are enough items to pay for the threads; results in
/// item order.
fn par_map<T: Send>(
    items: &[u32],
    workers: &mut [Witness],
    f: impl Fn(&mut Witness, u32) -> T + Sync,
) -> Vec<T> {
    if workers.len() == 1 || items.len() < PAR_MIN_ITEMS {
        let w = &mut workers[0];
        return items.iter().map(|&x| f(w, x)).collect();
    }
    let chunk = items.len().div_ceil(workers.len());
    let f = &f;
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .zip(workers.iter_mut())
            .map(|(part, w)| scope.spawn(move || part.iter().map(|&x| f(w, x)).collect::<Vec<T>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("contraction thread panicked"))
            .collect()
    })
}

/// Shortcuts added against arcs removed, plus the neighbours already
/// contracted and the depth, which spread the contraction evenly.
fn priority(
    out: &[Vec<(u32, u32)>],
    inn: &[Vec<(u32, u32)>],
    deleted: &[i32],
    level: &[i32],
    none: &[bool],
    ws: &mut Witness,
    x: u32,
) -> i32 {
    ws.settle = ESTIMATE_SETTLE;
    ws.shortcuts_of(out, inn, none, x);
    let xu = x as usize;
    let degree = (out[xu].len() + inn[xu].len()) as i32;
    4 * ws.found.len() as i32 - 2 * degree + 2 * deleted[xu] + level[xu]
}

impl Hierarchy {
    /// Contract the graph of `n` nodes and arcs `(from, to, seconds)` in
    /// rounds. Each round takes the remaining nodes whose priority is below
    /// that of every remaining neighbour (so no two are adjacent), finds
    /// their shortcuts in parallel with witness searches that avoid all of
    /// them, then removes them. A path through a node `x` of the round runs
    /// `u -> x -> v` with `u` and `v` outside the round, so it is replaced
    /// by a shortcut or by a witness that avoids the round: every time
    /// between the remaining nodes is kept. The result does not depend on
    /// the thread count. `None` when `cancel` is cancelled first (checked
    /// before every round).
    pub(super) fn build(
        n: usize,
        arcs: &[(u32, u32, u32)],
        n_threads: usize,
        cancel: &CancelToken,
    ) -> Option<Self> {
        if cancel.is_cancelled() {
            return None;
        }
        let mut out: Vec<Vec<(u32, u32)>> = vec![Vec::new(); n];
        let mut inn: Vec<Vec<(u32, u32)>> = vec![Vec::new(); n];
        for &(u, v, w) in arcs {
            if u != v {
                // a loop never shortens a path whose steps all cost >= 1
                add_arc(&mut out[u as usize], v, w);
                add_arc(&mut inn[v as usize], u, w);
            }
        }
        let mut workers: Vec<Witness> = (0..n_threads.max(1)).map(|_| Witness::new(n)).collect();
        let mut deleted = vec![0i32; n];
        let mut level = vec![0i32; n];
        let mut blocked = vec![false; n];
        let mut done = vec![false; n];
        let mut remaining: Vec<u32> = (0..n as u32).collect();
        let mut prio = par_map(&remaining, &mut workers, |ws, x| {
            priority(&out, &inn, &deleted, &level, &blocked, ws, x)
        });
        // Each node's arcs to the nodes still there when it goes: those all
        // rank higher.
        let mut up_arcs: Vec<Vec<(u32, u32)>> = vec![Vec::new(); n];
        let mut down_arcs: Vec<Vec<(u32, u32)>> = vec![Vec::new(); n];
        let mut order = Vec::with_capacity(n);
        let mut shortcuts = 0;
        let mut rounds = 0;
        let mut round: Vec<u32> = Vec::new();
        let mut dirty: Vec<u32> = Vec::new();
        let mut is_dirty = vec![false; n];
        while !remaining.is_empty() {
            if cancel.is_cancelled() {
                return None;
            }
            rounds += 1;
            let key = |x: u32| (prio[x as usize], x);
            round.clear();
            round.extend(remaining.iter().copied().filter(|&x| {
                let k = key(x);
                out[x as usize]
                    .iter()
                    .chain(&inn[x as usize])
                    .all(|&(y, _)| k < key(y))
            }));
            for &x in &round {
                blocked[x as usize] = true;
            }
            let found = par_map(&round, &mut workers, |ws, x| {
                ws.settle = WITNESS_SETTLE;
                ws.shortcuts_of(&out, &inn, &blocked, x);
                ws.found.clone()
            });
            dirty.clear();
            for (&x, sc) in round.iter().zip(found) {
                let xu = x as usize;
                blocked[xu] = false;
                done[xu] = true;
                order.push(x);
                up_arcs[xu] = std::mem::take(&mut out[xu]);
                down_arcs[xu] = std::mem::take(&mut inn[xu]);
                for &(v, _) in &up_arcs[xu] {
                    inn[v as usize].retain(|a| a.0 != x);
                }
                for &(u, _) in &down_arcs[xu] {
                    out[u as usize].retain(|a| a.0 != x);
                }
                for &(y, _) in up_arcs[xu].iter().chain(&down_arcs[xu]) {
                    let yu = y as usize;
                    deleted[yu] += 1;
                    level[yu] = level[yu].max(level[xu] + 1);
                    if !is_dirty[yu] {
                        is_dirty[yu] = true;
                        dirty.push(y);
                    }
                }
                for &(u, v, w) in &sc {
                    add_arc(&mut out[u as usize], v, w);
                    add_arc(&mut inn[v as usize], u, w);
                }
                shortcuts += sc.len();
            }
            remaining.retain(|&x| !done[x as usize]);
            let fresh = par_map(&dirty, &mut workers, |ws, y| {
                priority(&out, &inn, &deleted, &level, &blocked, ws, y)
            });
            for (&y, p) in dirty.iter().zip(fresh) {
                prio[y as usize] = p;
                is_dirty[y as usize] = false;
            }
        }
        // highest rank first: the last round's nodes get the lowest ids
        let mut id = vec![0u32; n];
        for (r, &x) in order.iter().rev().enumerate() {
            id[x as usize] = r as u32;
        }
        let mut up_off = Vec::with_capacity(n + 1);
        let mut down_off = Vec::with_capacity(n + 1);
        up_off.push(0);
        down_off.push(0);
        let mut up = Vec::new();
        let mut down = Vec::new();
        for &x in order.iter().rev() {
            let xu = x as usize;
            up.extend(up_arcs[xu].iter().map(|&(v, w)| (id[v as usize], w)));
            down.extend(down_arcs[xu].iter().map(|&(u, w)| (id[u as usize], w)));
            up_off.push(up.len() as u32);
            down_off.push(down.len() as u32);
        }
        Some(Hierarchy {
            id,
            up_off,
            up,
            down_off,
            down,
            shortcuts,
            rounds,
        })
    }

    pub(super) fn n(&self) -> usize {
        self.id.len()
    }

    /// Memory held, in bytes.
    pub(super) fn bytes(&self) -> usize {
        4 * (self.id.len() + self.up_off.len() + self.down_off.len())
            + 8 * (self.up.len() + self.down.len())
    }
}
