//! Isobands by marching squares, a port of the `isobander` of the R package
//! isoband 0.3.0 (Claus O. Wilke, MIT; `src/isoband.cpp`), the code behind
//! `isoband::isobands()`.
//!
//! Only the bands of isochrones are supported: from 0 up to a cutoff, over
//! values that are never negative, so a grid value is either inside the
//! band (`0 <= z < high`) or above it, and the 16 cell configurations of
//! those two states are the only ones that occur.
//!
//! isoband collects the rings by walking a `std::unordered_map` keyed by
//! grid points, so the order of the rings and the vertex each ring starts
//! at are those of the hash table, which depend on the C++ standard
//! library isoband was compiled with. [`HashTable`] reproduces both that R
//! packages are built with: LLVM's libc++ (macOS) and GCC's libstdc++
//! (Linux, and Windows through Rtools), chosen by [`CxxLibrary`]. With the
//! library of the platform the rings come out exactly as isoband gives
//! them there.

/// The C++ standard library whose `std::unordered_map` decides the order of
/// isoband's rings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CxxLibrary {
    /// LLVM's libc++, which R packages are compiled with on macOS.
    Libcxx,
    /// GCC's libstdc++ (GCC 11 to 15 lay the table out alike), which R
    /// packages are compiled with on Linux and, through Rtools, on Windows.
    Libstdcxx,
}

impl CxxLibrary {
    /// The library of the platform this crate is compiled for: libc++ on
    /// Apple targets, libstdc++ on every other native target, and libc++
    /// on WebAssembly (no R there to follow: fixed, so that the npm package
    /// gives the same polygons wherever it runs).
    pub const NATIVE: CxxLibrary = if cfg!(any(target_vendor = "apple", target_family = "wasm")) {
        CxxLibrary::Libcxx
    } else {
        CxxLibrary::Libstdcxx
    };
}

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

    /// isoband's `grid_point_hasher` through `std::hash<long long>`, which
    /// is the identity in libc++ and in libstdc++.
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
/// `__hash_table` and libstdc++'s `_Hashtable` lay it out. Both keep one
/// singly linked list of nodes, buckets pointing at the node before their
/// first one, put a new key first in its bucket (or first in the list when
/// its bucket is empty), take the bucket of a hash modulo the bucket count,
/// unlink erased nodes without moving the others and keep the bucket count
/// on `clear()`. Iteration follows the list. They differ in when and how
/// they rehash:
///
/// - libc++ starts with no buckets and, past a load factor of 1, grows to
///   `next_prime(2n + 1)` (powers of 2 aside), moving runs of nodes that
///   share a bucket together (`__do_rehash`);
/// - libstdc++ starts with 1 bucket and, past a load factor of 1, grows to
///   the next prime of its table at or above `max(size + 1, 2n)` (13 for
///   the first key; [`GNU_BUCKET_COUNTS`] holds the ones reached), relinking the nodes one by one at the front of their
///   new bucket or of the list (`_M_rehash` for unique keys, with
///   `_Prime_rehash_policy`).
#[derive(Debug, Clone)]
struct HashTable {
    lib: CxxLibrary,
    nodes: Vec<Node>,
    free: Vec<u32>,
    /// `anchor.next`.
    first: u32,
    buckets: Vec<u32>,
    size: usize,
    /// libstdc++ `_Prime_rehash_policy::_M_next_resize`: the size past
    /// which an insertion rehashes (0 before the first one).
    next_resize: usize,
}

/// libc++ `__constrain_hash`; equal to `h % bc`, which is libstdc++'s
/// `_Mod_range_hashing`.
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

/// libstdc++ `_Prime_rehash_policy::_M_need_rehash` (max load factor 1)
/// before the `n`-th key goes into a table of `bc` buckets: the new bucket
/// count, if it rehashes. `next_resize` is the policy's state, the size
/// past which it rehashes (0 before the first insertion).
///
/// The tables [`Isobander::new`] accepts never grow past
/// [`GNU_MAX_SIZE`] keys, so the next bucket count is always listed.
fn gnu_need_rehash(next_resize: &mut usize, bc: usize, n: usize) -> Option<usize> {
    if n <= *next_resize {
        return None;
    }
    let min_bkts = n.max(if *next_resize == 0 { 11 } else { 0 });
    if min_bkts < bc {
        *next_resize = bc;
        return None;
    }
    let nbc = gnu_next_bkt((min_bkts + 1).max(bc * 2))
        .expect("isoband: no more keys than Isobander::new checks for");
    *next_resize = nbc;
    Some(nbc)
}

/// libstdc++ `_Prime_rehash_policy::_M_next_bkt` for the arguments
/// insertions pass it (see [`GNU_BUCKET_COUNTS`]): the first listed count
/// at or above `n`, or `None` past the list.
fn gnu_next_bkt(n: usize) -> Option<usize> {
    let i = GNU_BUCKET_COUNTS.partition_point(|&p| p < n);
    GNU_BUCKET_COUNTS.get(i).copied()
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

/// The bucket counts a libstdc++ `std::unordered_map` that only grows by
/// insertion goes through, after its initial single bucket: 13 on the first
/// key, then, each time the size passes the bucket count, the policy's
/// next bucket count for twice the current one.
///
/// These are the values `_Prime_rehash_policy::_M_next_bkt` returns for
/// the only arguments isoband's map passes it (12 on the first insertion,
/// twice the bucket count afterwards; `clear()` and `erase()` keep the
/// bucket count, and isoband never calls `reserve()` or `rehash()`), so
/// they are the reachable entries of libstdc++'s list of primes, not the
/// list. Recorded from the real policy by `tests/data/isoband-hash/record.cpp`
/// (the `policy` line of `libstdcxx.txt`, GCC 16.2; the real containers of
/// that file, recorded with GCC 11 to 14 and 16, reach the counts up to
/// 712,697), which the tests check this list against. The list stops two
/// steps past what the largest grid needs (see [`gnu_max_keys`]).
const GNU_BUCKET_COUNTS: [usize; 23] = [
    13, 29, 59, 127, 257, 541, 1109, 2357, 5087, 10273, 20753, 42043, 85229, 172933, 351061,
    712697, 1447153, 2938679, 5967347, 12117689, 24607243, 49969847, 101473717,
];

/// The most keys the emulated libstdc++ table can take: past the last
/// listed bucket count, an insertion would rehash to a count the list does
/// not have.
const GNU_MAX_SIZE: usize = GNU_BUCKET_COUNTS[GNU_BUCKET_COUNTS.len() - 1];

/// An upper bound on the keys isoband's map holds for a grid of `cells`
/// points: each grid point and the crossings of the band's top on the edge
/// to its right and on the edge below it, so fewer than 3 per point. For
/// R5's largest grid (5,000,000 cells) that is 15,000,000 keys, which
/// needs the bucket count 24,607,243.
fn gnu_max_keys(cells: usize) -> usize {
    cells.saturating_mul(3)
}

/// Whether the hash table of `lib` can order the rings of a grid of
/// `cells` points as the real one does.
fn check_grid_size(cells: usize, lib: CxxLibrary) -> Result<(), String> {
    if lib == CxxLibrary::Libstdcxx && gnu_max_keys(cells) > GNU_MAX_SIZE {
        return Err(format!(
            "isochrone grid of {cells} cells is too large to order its polygons as \
             libstdc++ does (at most {} cells)",
            GNU_MAX_SIZE / 3
        ));
    }
    Ok(())
}

impl HashTable {
    fn new(lib: CxxLibrary) -> Self {
        HashTable {
            lib,
            nodes: Vec::new(),
            free: Vec::new(),
            first: NIL,
            // libstdc++ starts on its single bucket, libc++ on none
            buckets: match lib {
                CxxLibrary::Libcxx => Vec::new(),
                CxxLibrary::Libstdcxx => vec![NIL],
            },
            size: 0,
            next_resize: 0,
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
        let h = key.hash();
        match self.lib {
            CxxLibrary::Libcxx => self.grow_libcxx(),
            CxxLibrary::Libstdcxx => self.grow_libstdcxx(),
        }
        let bc = self.buckets.len();
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

    /// Before inserting a new key, libc++ (`__emplace_unique_key_args`).
    fn grow_libcxx(&mut self) {
        let bc = self.buckets.len();
        if (self.size + 1) as f32 > bc as f32 {
            let grow = 2 * bc + usize::from(!is_hash_power2(bc));
            let need = ((self.size + 1) as f32).ceil() as usize;
            self.rehash(grow.max(need));
        }
    }

    /// Before inserting a new key, libstdc++ (`_M_insert_unique_node` with
    /// `_Prime_rehash_policy::_M_need_rehash`, max load factor 1).
    fn grow_libstdcxx(&mut self) {
        let bc = self.buckets.len();
        if let Some(nbc) = gnu_need_rehash(&mut self.next_resize, bc, self.size + 1) {
            self.rehash_libstdcxx(nbc);
        }
    }

    /// libstdc++ `_M_rehash` for unique keys: every node, in list order,
    /// goes first in its new bucket, or first in the list when the bucket
    /// is new.
    fn rehash_libstdcxx(&mut self, nbc: usize) {
        let mut buckets = vec![NIL; nbc];
        let mut p = self.first;
        self.first = NIL;
        let mut bbegin = 0;
        while p != NIL {
            let next = self.nodes[p as usize].next;
            let b = constrain(self.nodes[p as usize].hash, nbc);
            if buckets[b] == NIL {
                self.nodes[p as usize].next = self.first;
                self.first = p;
                buckets[b] = ANCHOR;
                if self.nodes[p as usize].next != NIL {
                    buckets[bbegin] = p;
                }
                bbegin = b;
            } else {
                let head = buckets[b];
                let after = self.next_of(head);
                self.nodes[p as usize].next = after;
                self.set_next(head, p);
            }
            p = next;
        }
        self.buckets = buckets;
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
    /// An isobander whose rings come in the order isoband gives them when
    /// compiled with `lib`. Fails for a grid too large for the bucket
    /// counts of libstdc++ that are reproduced (more than 33 million
    /// points, far above R5's 5 million), rather than give another order.
    pub(crate) fn new(grid: Grid<'a>, lib: CxxLibrary) -> Result<Self, String> {
        debug_assert_eq!(grid.z.len(), grid.nrow() * grid.ncol());
        check_grid_size(grid.nrow().saturating_mul(grid.ncol()), lib)?;
        Ok(Isobander {
            grid,
            vhi: 0.0,
            map: HashTable::new(lib),
            tmp: Vec::with_capacity(8),
        })
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

    const LIBS: [CxxLibrary; 2] = [CxxLibrary::Libcxx, CxxLibrary::Libstdcxx];

    #[test]
    fn hash_table_matches_a_plain_map() {
        for lib in LIBS {
            matches_a_plain_map(lib);
        }
    }

    fn matches_a_plain_map(lib: CxxLibrary) {
        let mut t = HashTable::new(lib);
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

    /// splitmix64, as `tests/data/isoband-hash/record.cpp`.
    struct SplitMix(u64);

    impl SplitMix {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        }
    }

    /// FNV-1a 64 of the keys in iteration order, as `record.cpp`.
    fn fnv(t: &HashTable) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for k in t.keys() {
            for v in [k.r, k.c, i32::from(k.kind)] {
                for b in v.to_le_bytes() {
                    h ^= u64::from(b);
                    h = h.wrapping_mul(0x0100_0000_01b3);
                }
            }
        }
        h
    }

    fn key_list(t: &HashTable) -> String {
        t.keys()
            .iter()
            .map(|k| format!(" {},{},{}", k.r, k.c, k.kind))
            .collect()
    }

    /// Replays the operations of `record.cpp` on the emulated table and
    /// checks every state the real `std::unordered_map` recorded.
    fn replay(lib: CxxLibrary, fixture: &str) {
        let mut lines = fixture.lines().filter(|l| !l.is_empty());
        let mut sequences = 0;
        let mut policies = 0;
        while let Some(head) = lines.next() {
            let f: Vec<&str> = head.split(' ').collect();
            if f[0] == "grow" {
                let want: Vec<usize> = f[1..].iter().map(|v| v.parse().unwrap()).collect();
                let mut t = HashTable::new(lib);
                let mut got = vec![t.buckets.len()];
                for i in 0..400_000 {
                    let k = GridPoint {
                        r: i / 700,
                        c: i % 700,
                        kind: GRID,
                    };
                    t.set(k, Connect::default());
                    if got.last() != Some(&t.buckets.len()) {
                        got.push(t.buckets.len());
                    }
                }
                assert_eq!(got, want, "{lib:?}: bucket counts");
                continue;
            }
            if f[0] == "policy" {
                // libstdc++'s own policy, past the keys any grid holds
                let want: Vec<usize> = f[1..].iter().map(|v| v.parse().unwrap()).collect();
                assert_eq!(gnu_policy_counts(60_000_000), want, "{lib:?}: policy");
                policies += 1;
                continue;
            }
            assert_eq!(f[0], "seq", "{lib:?}: fixture line {head}");
            let n: Vec<u64> = f[1..].iter().map(|v| v.parse().unwrap()).collect();
            let (seed, phases, ops, rows, cols) = (n[0], n[1], n[2], n[3], n[4]);
            let mut rng = SplitMix(seed);
            let mut t = HashTable::new(lib);
            for _ in 0..phases {
                t.clear();
                for i in 0..ops {
                    let op = rng.next() % 100;
                    let k = GridPoint {
                        r: (rng.next() % rows) as i32,
                        c: (rng.next() % cols) as i32,
                        kind: (rng.next() % 5) as u8,
                    };
                    if op < 65 {
                        let mut v = Connect::default();
                        v.prev.r = i as i32;
                        t.set(k, v);
                    } else {
                        t.erase(k);
                    }
                    if i % 256 == 255 {
                        let want = lines.next().unwrap();
                        let got = format!("cp {} {} {}", t.buckets.len(), t.size, fnv(&t));
                        assert_eq!(got, want, "{lib:?}: seq {seed}, phase op {i}");
                    }
                }
            }
            let want = lines.next().unwrap();
            let got = if t.size <= 1500 {
                format!("end {} {}{}", t.buckets.len(), t.size, key_list(&t))
            } else {
                format!("end {} {} fnv {}", t.buckets.len(), t.size, fnv(&t))
            };
            assert_eq!(got, want, "{lib:?}: seq {seed}, end");
            sequences += 1;
        }
        assert_eq!(sequences, 8);
        assert_eq!(policies, usize::from(lib == CxxLibrary::Libstdcxx));
    }

    #[test]
    fn iteration_order_is_that_of_libcxx() {
        // recorded with Apple clang 21 (libc++) on macOS 26
        replay(
            CxxLibrary::Libcxx,
            include_str!("../../tests/data/isoband-hash/libcxx.txt"),
        );
    }

    #[test]
    fn iteration_order_is_that_of_libstdcxx() {
        // recorded with g++ 11.5, 12.4, 13.3 and 14.2 (identical output),
        // and again, with the `policy` line, with g++ 16.2 (identical)
        replay(
            CxxLibrary::Libstdcxx,
            include_str!("../../tests/data/isoband-hash/libstdcxx.txt"),
        );
    }

    /// The bucket counts of the emulated policy, one key at a time from
    /// an empty table up to `keys` keys, as `record.cpp` asks the real one.
    fn gnu_policy_counts(keys: usize) -> Vec<usize> {
        let (mut bc, mut next_resize) = (1, 0);
        let mut got = vec![bc];
        for n in 0..keys {
            if n < next_resize {
                continue; // `n + 1 <= next_resize`: no rehash, no state change
            }
            if let Some(nbc) = gnu_need_rehash(&mut next_resize, bc, n + 1) {
                bc = nbc;
                got.push(bc);
            }
        }
        got
    }

    #[test]
    fn libstdcxx_bucket_counts() {
        // the list is the counts the policy reaches, in order, each one
        let mut want = vec![1];
        want.extend(GNU_BUCKET_COUNTS);
        assert_eq!(gnu_policy_counts(GNU_MAX_SIZE), want);
        assert!(GNU_BUCKET_COUNTS.windows(2).all(|w| w[1] >= 2 * w[0]));
        // the keys of R5's largest grid (5,000,000 cells) fit two counts
        // before the end of the list
        let largest = gnu_max_keys(5_000_000);
        let needed = GNU_BUCKET_COUNTS.partition_point(|&p| p < largest);
        assert_eq!(GNU_BUCKET_COUNTS[needed], 24_607_243);
        assert_eq!(needed, GNU_BUCKET_COUNTS.len() - 3);
        // past the list there is no count, rather than a wrong one
        assert_eq!(gnu_next_bkt(GNU_MAX_SIZE + 1), None);
    }

    #[test]
    fn libstdcxx_refuses_a_grid_past_the_list() {
        let most = GNU_MAX_SIZE / 3;
        assert!(check_grid_size(5_000_000, CxxLibrary::Libstdcxx).is_ok());
        assert!(check_grid_size(most, CxxLibrary::Libstdcxx).is_ok());
        let err = check_grid_size(most + 1, CxxLibrary::Libstdcxx).unwrap_err();
        assert!(err.contains("too large"), "{err}");
        assert!(check_grid_size(most + 1, CxxLibrary::Libcxx).is_ok());
    }

    #[test]
    fn native_library() {
        let want = if cfg!(any(
            target_os = "macos",
            target_os = "ios",
            target_family = "wasm"
        )) {
            CxxLibrary::Libcxx
        } else {
            CxxLibrary::Libstdcxx
        };
        assert_eq!(CxxLibrary::NATIVE, want);
    }

    #[test]
    fn a_single_cell_peak() {
        for lib in LIBS {
            single_cell_peak(lib);
        }
    }

    fn single_cell_peak(lib: CxxLibrary) {
        // 3 x 3 grid, low in the middle: one ring around the centre
        let x = [0.0, 1.0, 2.0];
        let y = [0.0, 1.0, 2.0];
        let z = [9.0, 9.0, 9.0, 9.0, 1.0, 9.0, 9.0, 9.0, 9.0];
        let mut b = Isobander::new(
            Grid {
                x: &x,
                y: &y,
                z: &z,
            },
            lib,
        )
        .unwrap();
        let rings = b.band(5.0);
        assert_eq!(rings.len(), 1);
        assert_eq!(rings[0].len(), 4);
        let mut pts = rings[0].clone();
        pts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(pts, vec![[0.5, 1.0], [1.0, 0.5], [1.0, 1.5], [1.5, 1.0]]);
        assert!(b.band(0.5).is_empty());
    }
}
