//! Isobands by marching squares, a port of the `isobander` of the R package
//! isoband 0.3.0 (Claus O. Wilke, MIT; `src/isoband.cpp`), which r5r's
//! `isochrone()` calls through `isoband::isobands()`.
//!
//! Only the bands r5r asks for are supported: from 0 up to a cutoff, over
//! values that are never negative, so a grid value is either inside the
//! band (`0 <= z < high`) or above it, and the 16 cell configurations of
//! those two states are the only ones that occur.
//!
//! isoband collects the rings by walking a `std::unordered_map` keyed by
//! grid points, so the order of the rings and the vertex each ring starts
//! at are those of the hash table. [`HashTable`] reproduces the table of
//! LLVM's libc++ (the C++ library R packages are built with on macOS):
//! with it the rings come out exactly as isoband gives them there.

/// Kind of a polygon vertex in grid space (isoband `point_type`).
const GRID: u8 = 0;
/// Intersection with a horizontal edge at the high value (`hintersect_hi`).
const H_HI: u8 = 2;
/// Intersection with a vertical edge at the high value (`vintersect_hi`).
const V_HI: u8 = 4;

/// A vertex in grid space: row, column and kind (isoband `grid_point`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct GridPoint {
    r: i32,
    c: i32,
    kind: u8,
}

impl GridPoint {
    /// The default `grid_point()`: off the grid.
    const NONE: GridPoint = GridPoint {
        r: -1,
        c: -1,
        kind: GRID,
    };

    /// isoband's `grid_point_hasher` through libc++'s `std::hash<long
    /// long>`, which is the identity.
    fn hash(self) -> u64 {
        ((i64::from(self.r) << 30) ^ (i64::from(self.c) << 3) ^ i64::from(self.kind)) as u64
    }
}

/// Connections of a vertex (isoband `point_connect`).
#[derive(Debug, Clone, Copy)]
struct Connect {
    prev: GridPoint,
    next: GridPoint,
    prev2: GridPoint,
    next2: GridPoint,
    altpoint: bool,
    collected: bool,
    collected2: bool,
}

impl Default for Connect {
    fn default() -> Self {
        Connect {
            prev: GridPoint::NONE,
            next: GridPoint::NONE,
            prev2: GridPoint::NONE,
            next2: GridPoint::NONE,
            altpoint: false,
            collected: false,
            collected2: false,
        }
    }
}

/// No node / empty bucket.
const NIL: u32 = u32::MAX;
/// The list anchor ("before begin" node of libc++).
const ANCHOR: u32 = u32::MAX - 1;

#[derive(Debug, Clone)]
struct Node {
    key: GridPoint,
    hash: u64,
    value: Connect,
    next: u32,
}

/// `std::unordered_map<grid_point, point_connect>` as libc++'s
/// `__hash_table` lays it out: one singly linked list of nodes, buckets
/// pointing at the node before their first one, prime bucket counts grown
/// to `next_prime(2n + 1)` past a load factor of 1, new keys put first in
/// their bucket, `clear()` keeping the bucket count. Iteration follows the
/// list, so it visits the keys in libc++'s order.
#[derive(Debug, Clone, Default)]
struct HashTable {
    nodes: Vec<Node>,
    free: Vec<u32>,
    /// `anchor.next`.
    first: u32,
    buckets: Vec<u32>,
    size: usize,
}

/// libc++ `__constrain_hash`.
fn constrain(h: u64, bc: usize) -> usize {
    let bc = bc as u64;
    (if bc & (bc - 1) == 0 {
        h & (bc - 1)
    } else if h < bc {
        h
    } else {
        h % bc
    }) as usize
}

/// libc++ `__is_hash_power2`.
fn is_hash_power2(bc: usize) -> bool {
    bc > 2 && bc & (bc - 1) == 0
}

/// libc++ `__next_prime`: the smallest prime at or above `n`.
fn next_prime(n: usize) -> usize {
    fn is_prime(n: usize) -> bool {
        if n < 2 {
            return false;
        }
        let mut d = 2;
        while d * d <= n {
            if n % d == 0 {
                return false;
            }
            d += 1;
        }
        true
    }
    let mut n = n;
    while !is_prime(n) {
        n += 1;
    }
    n
}

impl HashTable {
    fn new() -> Self {
        HashTable {
            first: NIL,
            ..Default::default()
        }
    }

    fn next_of(&self, p: u32) -> u32 {
        if p == ANCHOR {
            self.first
        } else {
            self.nodes[p as usize].next
        }
    }

    fn set_next(&mut self, p: u32, v: u32) {
        if p == ANCHOR {
            self.first = v;
        } else {
            self.nodes[p as usize].next = v;
        }
    }

    fn find(&self, key: GridPoint) -> Option<u32> {
        let bc = self.buckets.len();
        if bc == 0 || self.size == 0 {
            return None;
        }
        let h = key.hash();
        let ch = constrain(h, bc);
        let pn = self.buckets[ch];
        if pn == NIL {
            return None;
        }
        let mut nd = self.next_of(pn);
        while nd != NIL {
            let n = &self.nodes[nd as usize];
            if !(n.hash == h || constrain(n.hash, bc) == ch) {
                break;
            }
            if n.hash == h && n.key == key {
                return Some(nd);
            }
            nd = n.next;
        }
        None
    }

    fn get(&self, key: GridPoint) -> &Connect {
        let i = self.find(key).expect("isoband: point in the polygon grid");
        &self.nodes[i as usize].value
    }

    fn get_mut(&mut self, key: GridPoint) -> &mut Connect {
        let i = self.find(key).expect("isoband: point in the polygon grid");
        &mut self.nodes[i as usize].value
    }

    /// `map[key] = value`.
    fn set(&mut self, key: GridPoint, value: Connect) {
        if let Some(i) = self.find(key) {
            self.nodes[i as usize].value = value;
            return;
        }
        // __emplace_unique_key_args
        let h = key.hash();
        let mut bc = self.buckets.len();
        if (self.size + 1) as f32 > bc as f32 {
            let grow = 2 * bc + usize::from(!is_hash_power2(bc));
            let need = ((self.size + 1) as f32).ceil() as usize;
            self.rehash(grow.max(need));
            bc = self.buckets.len();
        }
        let ch = constrain(h, bc);
        let node = Node {
            key,
            hash: h,
            value,
            next: NIL,
        };
        let nd = match self.free.pop() {
            Some(i) => {
                self.nodes[i as usize] = node;
                i
            }
            None => {
                self.nodes.push(node);
                (self.nodes.len() - 1) as u32
            }
        };
        let pn = self.buckets[ch];
        if pn == NIL {
            let after = self.first;
            self.nodes[nd as usize].next = after;
            self.first = nd;
            self.buckets[ch] = ANCHOR;
            if after != NIL {
                let nh = constrain(self.nodes[after as usize].hash, bc);
                self.buckets[nh] = nd;
            }
        } else {
            let after = self.next_of(pn);
            self.nodes[nd as usize].next = after;
            self.set_next(pn, nd);
        }
        self.size += 1;
    }

    /// libc++ `__rehash`, growing only (as insertions call it).
    fn rehash(&mut self, n: usize) {
        let n = if n == 1 {
            2
        } else if n & (n - 1) != 0 {
            next_prime(n)
        } else {
            n
        };
        if n > self.buckets.len() {
            self.do_rehash(n);
        }
    }

    /// libc++ `__do_rehash` for unique keys.
    fn do_rehash(&mut self, nbc: usize) {
        self.buckets = vec![NIL; nbc];
        let mut pp = ANCHOR;
        let mut cp = self.first;
        if cp == NIL {
            return;
        }
        let mut phash = constrain(self.nodes[cp as usize].hash, nbc);
        self.buckets[phash] = pp;
        pp = cp;
        cp = self.next_of(pp);
        while cp != NIL {
            let ch = constrain(self.nodes[cp as usize].hash, nbc);
            if ch == phash {
                pp = cp;
            } else if self.buckets[ch] == NIL {
                self.buckets[ch] = pp;
                pp = cp;
                phash = ch;
            } else {
                let np = cp;
                let after_np = self.nodes[np as usize].next;
                self.set_next(pp, after_np);
                let head = self.buckets[ch];
                let after_head = self.next_of(head);
                self.nodes[np as usize].next = after_head;
                self.set_next(head, cp);
            }
            cp = self.next_of(pp);
        }
    }

    /// `map.erase(key)` (libc++ `remove`).
    fn erase(&mut self, key: GridPoint) {
        let Some(cn) = self.find(key) else { return };
        let bc = self.buckets.len();
        let ch = constrain(self.nodes[cn as usize].hash, bc);
        let mut pn = self.buckets[ch];
        while self.next_of(pn) != cn {
            pn = self.next_of(pn);
        }
        let cn_next = self.nodes[cn as usize].next;
        let same_bucket =
            |t: &Self, p: u32| p != NIL && constrain(t.nodes[p as usize].hash, bc) == ch;
        // the bucket empties when neither neighbour is in it
        if (pn == ANCHOR || constrain(self.nodes[pn as usize].hash, bc) != ch)
            && !same_bucket(self, cn_next)
        {
            self.buckets[ch] = NIL;
        }
        if cn_next != NIL {
            let nh = constrain(self.nodes[cn_next as usize].hash, bc);
            if nh != ch {
                self.buckets[nh] = pn;
            }
        }
        self.set_next(pn, cn_next);
        self.nodes[cn as usize].next = NIL;
        self.free.push(cn);
        self.size -= 1;
    }

    /// `map.clear()`: the bucket count stays.
    fn clear(&mut self) {
        if self.size > 0 {
            self.nodes.clear();
            self.free.clear();
            self.first = NIL;
            self.buckets.iter_mut().for_each(|b| *b = NIL);
            self.size = 0;
        }
    }

    /// Keys in iteration order.
    fn keys(&self) -> Vec<GridPoint> {
        let mut out = Vec::with_capacity(self.size);
        let mut p = self.first;
        while p != NIL {
            out.push(self.nodes[p as usize].key);
            p = self.nodes[p as usize].next;
        }
        out
    }
}

/// The values of a grid, row by row, with the coordinates of its columns
/// and rows: isoband's `x` (one per column), `y` (one per row) and `z`.
pub(crate) struct Grid<'a> {
    pub(crate) x: &'a [f64],
    pub(crate) y: &'a [f64],
    /// `z[r * ncol + c]`.
    pub(crate) z: &'a [f64],
}

impl Grid<'_> {
    fn ncol(&self) -> usize {
        self.x.len()
    }

    fn nrow(&self) -> usize {
        self.y.len()
    }

    fn z(&self, r: i32, c: i32) -> f64 {
        self.z[r as usize * self.ncol() + c as usize]
    }
}

/// The isobander of one grid, reused band after band as isoband does (the
/// hash table keeps its bucket count from one band to the next, which
/// decides the order of the rings).
pub(crate) struct Isobander<'a> {
    grid: Grid<'a>,
    vhi: f64,
    map: HashTable,
    tmp: Vec<GridPoint>,
}

/// A ring in grid coordinates (x, y), not closed: the last point is not
/// repeated.
pub(crate) type Ring = Vec<[f64; 2]>;

impl<'a> Isobander<'a> {
    pub(crate) fn new(grid: Grid<'a>) -> Self {
        debug_assert_eq!(grid.z.len(), grid.nrow() * grid.ncol());
        Isobander {
            grid,
            vhi: 0.0,
            map: HashTable::new(),
            tmp: Vec::with_capacity(8),
        }
    }

    /// The rings of the band `[0, high)`, in isoband's order. Every value
    /// must be finite and at least 0.
    pub(crate) fn band(&mut self, high: f64) -> Vec<Ring> {
        self.vhi = high;
        self.calculate_contour();
        self.collect()
    }

    fn poly(&mut self, pts: &[(i32, i32, u8)]) {
        self.tmp.clear();
        self.tmp
            .extend(pts.iter().map(|&(r, c, kind)| GridPoint { r, c, kind }));
        self.poly_merge();
    }

    /// isoband `poly_merge`.
    fn poly_merge(&mut self) {
        let n = self.tmp.len();
        let mut to_delete = [false; 8];
        let mut conn = [Connect::default(); 8];
        for i in 0..n {
            conn[i].altpoint = false;
            conn[i].next = self.tmp[if i + 1 < n { i + 1 } else { 0 }];
            conn[i].prev = self.tmp[if i >= 1 { i - 1 } else { n - 1 }];
            let p = self.tmp[i];
            if let Some(idx) = self.map.find(p) {
                let g = self.map.nodes[idx as usize].value;
                let t = &mut conn[i];
                if !g.altpoint {
                    let score = 2 * u8::from(t.next == g.prev) + u8::from(t.prev == g.next);
                    match score {
                        3 => to_delete[i] = true,
                        2 => t.next = g.next,
                        1 => t.prev = g.prev,
                        _ => {
                            t.prev2 = g.prev;
                            t.next2 = g.next;
                            t.altpoint = true;
                        }
                    }
                } else {
                    let score = 8 * u8::from(t.next == g.prev2)
                        + 4 * u8::from(t.prev == g.next2)
                        + 2 * u8::from(t.next == g.prev)
                        + u8::from(t.prev == g.next);
                    match score {
                        9 => {
                            t.next = g.next2;
                            t.prev = g.prev;
                        }
                        6 => {
                            t.next = g.next;
                            t.prev = g.prev2;
                        }
                        8 => {
                            t.next2 = g.next2;
                            t.prev2 = t.prev;
                            t.prev = g.prev;
                            t.next = g.next;
                            t.altpoint = true;
                        }
                        4 => {
                            t.prev2 = g.prev2;
                            t.next2 = t.next;
                            t.prev = g.prev;
                            t.next = g.next;
                            t.altpoint = true;
                        }
                        2 => {
                            t.next = g.next;
                            t.prev2 = g.prev2;
                            t.next2 = g.next2;
                            t.altpoint = true;
                        }
                        1 => {
                            t.prev = g.prev;
                            t.prev2 = g.prev2;
                            t.next2 = g.next2;
                            t.altpoint = true;
                        }
                        // isoband stops with "undefined merging configuration"
                        _ => panic!("isoband: undefined merging configuration {score}"),
                    }
                }
            }
        }
        for i in 0..n {
            let p = self.tmp[i];
            if to_delete[i] {
                self.map.erase(p);
            } else {
                self.map.set(p, conn[i]);
            }
        }
    }

    /// isoband `calculate_contour` for a band from 0 (values never below).
    fn calculate_contour(&mut self) {
        self.map.clear();
        let (nrow, ncol) = (self.grid.nrow() as i32, self.grid.ncol() as i32);
        let vhi = self.vhi;
        // ternarized: 1 inside the band, 2 at or above its top
        let t = |z: f64| -> u8 {
            debug_assert!(z.is_finite() && z >= 0.0);
            if z >= vhi {
                2
            } else {
                1
            }
        };
        for r in 0..nrow - 1 {
            for c in 0..ncol - 1 {
                let g = &self.grid;
                let (z_tl, z_tr, z_br, z_bl) =
                    (g.z(r, c), g.z(r, c + 1), g.z(r + 1, c + 1), g.z(r + 1, c));
                let index = 27 * t(z_tl) + 9 * t(z_tr) + 3 * t(z_br) + t(z_bl);
                // all polygons are drawn clockwise for proper merging
                match index {
                    80 => {}
                    // single triangle
                    79 => self.poly(&[(r, c, V_HI), (r + 1, c, H_HI), (r + 1, c, GRID)]),
                    77 => self.poly(&[(r, c + 1, V_HI), (r + 1, c + 1, GRID), (r + 1, c, H_HI)]),
                    71 => self.poly(&[(r, c, H_HI), (r, c + 1, GRID), (r, c + 1, V_HI)]),
                    53 => self.poly(&[(r, c, V_HI), (r, c, GRID), (r, c, H_HI)]),
                    // single rectangle
                    76 => self.poly(&[
                        (r, c, V_HI),
                        (r, c + 1, V_HI),
                        (r + 1, c + 1, GRID),
                        (r + 1, c, GRID),
                    ]),
                    68 => self.poly(&[
                        (r, c, H_HI),
                        (r, c + 1, GRID),
                        (r + 1, c + 1, GRID),
                        (r + 1, c, H_HI),
                    ]),
                    44 => self.poly(&[
                        (r, c, GRID),
                        (r, c + 1, GRID),
                        (r, c + 1, V_HI),
                        (r, c, V_HI),
                    ]),
                    52 => self.poly(&[
                        (r, c, H_HI),
                        (r + 1, c, H_HI),
                        (r + 1, c, GRID),
                        (r, c, GRID),
                    ]),
                    // single square
                    40 => self.poly(&[
                        (r, c, GRID),
                        (r, c + 1, GRID),
                        (r + 1, c + 1, GRID),
                        (r + 1, c, GRID),
                    ]),
                    // single pentagon
                    49 => self.poly(&[
                        (r, c, GRID),
                        (r, c, H_HI),
                        (r, c + 1, V_HI),
                        (r + 1, c + 1, GRID),
                        (r + 1, c, GRID),
                    ]),
                    67 => self.poly(&[
                        (r + 1, c, GRID),
                        (r, c, V_HI),
                        (r, c, H_HI),
                        (r, c + 1, GRID),
                        (r + 1, c + 1, GRID),
                    ]),
                    41 => self.poly(&[
                        (r, c, GRID),
                        (r, c + 1, GRID),
                        (r + 1, c + 1, GRID),
                        (r + 1, c, H_HI),
                        (r, c, V_HI),
                    ]),
                    43 => self.poly(&[
                        (r, c, GRID),
                        (r, c + 1, GRID),
                        (r, c + 1, V_HI),
                        (r + 1, c, H_HI),
                        (r + 1, c, GRID),
                    ]),
                    // 6-sided saddles
                    70 => {
                        if central_value(z_tl, z_tr, z_bl, z_br) >= vhi {
                            self.poly(&[(r + 1, c, GRID), (r, c, V_HI), (r + 1, c, H_HI)]);
                            self.poly(&[(r, c + 1, GRID), (r, c + 1, V_HI), (r, c, H_HI)]);
                        } else {
                            self.poly(&[
                                (r + 1, c, GRID),
                                (r, c, V_HI),
                                (r, c, H_HI),
                                (r, c + 1, GRID),
                                (r, c + 1, V_HI),
                                (r + 1, c, H_HI),
                            ]);
                        }
                    }
                    50 => {
                        if central_value(z_tl, z_tr, z_bl, z_br) >= vhi {
                            self.poly(&[(r, c, GRID), (r, c, H_HI), (r, c, V_HI)]);
                            self.poly(&[(r + 1, c + 1, GRID), (r + 1, c, H_HI), (r, c + 1, V_HI)]);
                        } else {
                            self.poly(&[
                                (r, c, GRID),
                                (r, c, H_HI),
                                (r, c + 1, V_HI),
                                (r + 1, c + 1, GRID),
                                (r + 1, c, H_HI),
                                (r, c, V_HI),
                            ]);
                        }
                    }
                    _ => unreachable!("cell index {index} with values never below 0"),
                }
            }
        }
    }

    /// isoband `calc_point_coords` (upper level only).
    fn coords(&self, p: GridPoint) -> [f64; 2] {
        let g = &self.grid;
        let (r, c) = (p.r as usize, p.c as usize);
        match p.kind {
            GRID => [g.x[c], g.y[r]],
            H_HI => [
                interpolate(
                    g.x[c],
                    g.x[c + 1],
                    g.z(p.r, p.c),
                    g.z(p.r, p.c + 1),
                    self.vhi,
                ),
                g.y[r],
            ],
            V_HI => [
                g.x[c],
                interpolate(
                    g.y[r],
                    g.y[r + 1],
                    g.z(p.r, p.c),
                    g.z(p.r + 1, p.c),
                    self.vhi,
                ),
            ],
            _ => unreachable!("only the upper level is interpolated"),
        }
    }

    /// isoband `collect`: walk the hash table, following each ring.
    fn collect(&mut self) -> Vec<Ring> {
        let mut rings = Vec::new();
        for start in self.map.keys() {
            let s = *self.map.get(start);
            if (s.collected && !s.altpoint) || (s.collected && s.collected2 && s.altpoint) {
                continue;
            }
            let mut ring = Vec::new();
            let mut cur = start;
            let mut prev = if s.altpoint && !s.collected2 {
                s.prev2
            } else {
                s.prev
            };
            loop {
                ring.push(self.coords(cur));
                let pc = self.map.get_mut(cur);
                let newcur = if pc.altpoint && pc.prev2 == prev {
                    pc.collected2 = true;
                    pc.next2
                } else {
                    pc.collected = true;
                    pc.next
                };
                prev = cur;
                cur = newcur;
                if cur == start {
                    break;
                }
            }
            rings.push(ring);
        }
        rings
    }
}

/// isoband `central_value`: `(z[r,c] + z[r,c+1] + z[r+1,c] + z[r+1,c+1]) / 4`.
fn central_value(z_tl: f64, z_tr: f64, z_bl: f64, z_br: f64) -> f64 {
    (z_tl + z_tr + z_bl + z_br) / 4.0
}

/// isoband `interpolate`: `x0 + d * (x1 - x0)`, `d = (value - z0) / (z1 - z0)`.
fn interpolate(x0: f64, x1: f64, z0: f64, z1: f64, value: f64) -> f64 {
    let d = (value - z0) / (z1 - z0);
    x0 + d * (x1 - x0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_table_matches_a_plain_map() {
        let mut t = HashTable::new();
        let mut reference = std::collections::HashMap::new();
        let mut x = 0x2545_f491_4f6c_dd1du64;
        for i in 0..20_000 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let k = GridPoint {
                r: (x % 97) as i32,
                c: ((x >> 8) % 89) as i32,
                kind: ((x >> 16) % 5) as u8,
            };
            if i % 3 == 0 {
                t.erase(k);
                reference.remove(&(k.r, k.c, k.kind));
            } else {
                let mut v = Connect::default();
                v.prev.r = i;
                t.set(k, v);
                reference.insert((k.r, k.c, k.kind), i);
            }
            if i % 5000 == 0 {
                t.clear();
                reference.clear();
            }
        }
        let keys = t.keys();
        assert_eq!(keys.len(), reference.len());
        assert_eq!(t.size, reference.len());
        for k in keys {
            assert_eq!(t.get(k).prev.r, reference[&(k.r, k.c, k.kind)]);
        }
        // every bucket's chain holds only its own keys, contiguously
        let bc = t.buckets.len();
        let order: Vec<usize> = t.keys().iter().map(|k| constrain(k.hash(), bc)).collect();
        let mut seen = std::collections::HashSet::new();
        for w in order.windows(2) {
            if w[0] != w[1] {
                assert!(seen.insert(w[0]), "bucket {} split", w[0]);
            }
        }
    }

    #[test]
    fn bucket_counts_grow_as_libcxx() {
        let mut t = HashTable::new();
        let mut counts = Vec::new();
        for i in 0..200 {
            t.set(
                GridPoint {
                    r: i,
                    c: 0,
                    kind: 0,
                },
                Connect::default(),
            );
            if counts.last() != Some(&t.buckets.len()) {
                counts.push(t.buckets.len());
            }
        }
        assert_eq!(counts, vec![2, 5, 11, 23, 47, 97, 197, 397]);
    }

    #[test]
    fn a_single_cell_peak() {
        // 3 x 3 grid, low in the middle: one ring around the centre
        let x = [0.0, 1.0, 2.0];
        let y = [0.0, 1.0, 2.0];
        let z = [9.0, 9.0, 9.0, 9.0, 1.0, 9.0, 9.0, 9.0, 9.0];
        let mut b = Isobander::new(Grid {
            x: &x,
            y: &y,
            z: &z,
        });
        let rings = b.band(5.0);
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].len(), 4);
        let mut pts = rings[0].clone();
        pts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(pts, vec![[0.5, 1.0], [1.0, 0.5], [1.0, 1.5], [1.5, 1.0]]);
        assert!(b.band(0.5).is_empty());
    }
}
