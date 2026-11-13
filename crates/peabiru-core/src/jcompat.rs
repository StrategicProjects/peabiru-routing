//! Bit-exact stand-ins for the Java library behaviour R5 results depend on:
//! the commons-math3 3.0 Mersenne Twister and `BitsStreamGenerator.nextInt`,
//! `java.util.Objects.hash`, and the slot layout (hence iteration order) of
//! Trove 3 open-addressing int hash maps. Checked against the R5 7.6 jar.
//!
//! [`MersenneTwister`] is a translation to Rust, by the peabiru authors, of
//! parts of `org.apache.commons.math3.random.MersenneTwister` and
//! `BitsStreamGenerator` of Apache Commons Math 3.0 (Copyright 2001-2012 The
//! Apache Software Foundation; Apache License, Version 2.0), themselves
//! translated from the C generator of Makoto Matsumoto and Takuji Nishimura
//! (Copyright (C) 1997 - 2002, BSD 3-clause). The prime table comes from
//! Trove's `PrimeFinder`, a Colt file (Copyright (c) 1999 CERN). The rest is
//! this project's own code. Licences and notices: `LICENSE-THIRD-PARTY`.

/// Mersenne Twister MT19937 as commons-math3 3.0 `MersenneTwister`.
pub struct MersenneTwister {
    mt: [u32; 624],
    mti: usize,
}

const N: usize = 624;
const M: usize = 397;

impl MersenneTwister {
    fn init_genrand(s: u32) -> [u32; N] {
        let mut mt = [0u32; N];
        mt[0] = s;
        for i in 1..N {
            mt[i] = 1_812_433_253u32
                .wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        mt
    }

    /// `setSeed(int[])`, the reference `init_by_array`.
    pub fn from_array(key: &[u32]) -> Self {
        let mut mt = Self::init_genrand(19_650_218);
        let (mut i, mut j) = (1usize, 0usize);
        for _ in 0..N.max(key.len()) {
            mt[i] = (mt[i] ^ (mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1_664_525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                mt[0] = mt[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            mt[i] = (mt[i] ^ (mt[i - 1] ^ (mt[i - 1] >> 30)).wrapping_mul(1_566_083_941))
                .wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                mt[0] = mt[N - 1];
                i = 1;
            }
        }
        mt[0] = 0x8000_0000;
        MersenneTwister { mt, mti: N }
    }

    /// `setSeed(long)`: the long split into its high and low int words.
    pub fn from_long(seed: i64) -> Self {
        Self::from_array(&[(seed as u64 >> 32) as u32, seed as u32])
    }

    fn next_u32(&mut self) -> u32 {
        if self.mti >= N {
            for k in 0..N {
                let y = (self.mt[k] & 0x8000_0000) | (self.mt[(k + 1) % N] & 0x7fff_ffff);
                let mag = if y & 1 == 0 { 0 } else { 0x9908_b0df };
                self.mt[k] = self.mt[(k + M) % N] ^ (y >> 1) ^ mag;
            }
            self.mti = 0;
        }
        let mut y = self.mt[self.mti];
        self.mti += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// `next(bits)`.
    fn next(&mut self, bits: u32) -> i32 {
        (self.next_u32() >> (32 - bits)) as i32
    }

    /// `nextInt(n)` of commons-math3 3.0 `BitsStreamGenerator`, `n > 0`.
    pub fn next_int(&mut self, n: i32) -> i32 {
        assert!(n > 0, "nextInt bound must be positive");
        if n & n.wrapping_neg() == n {
            return ((i64::from(n) * i64::from(self.next(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let val = bits % n;
            if bits.wrapping_sub(val).wrapping_add(n - 1) >= 0 {
                return val;
            }
        }
    }
}

/// `Double.hashCode`.
pub(crate) fn double_hash(x: f64) -> i32 {
    let b = x.to_bits();
    (b ^ (b >> 32)) as i32
}

/// `java.util.Objects.hash` of values whose own `hashCode`s are `hashes`.
pub(crate) fn objects_hash(hashes: &[i32]) -> i32 {
    hashes
        .iter()
        .fold(1i32, |h, &x| h.wrapping_mul(31).wrapping_add(x))
}

/// `String.hashCode` (over UTF-16 code units); `None`, Java's `null`, is 0
/// as `java.util.HashMap` hashes it.
pub(crate) fn string_hash(s: Option<&str>) -> i32 {
    s.map_or(0, |s| {
        s.encode_utf16()
            .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)))
    })
}

/// The bucket layout of a `java.util.HashMap` (or `HashSet`) built by
/// insertions only, enough to reproduce its iteration order: buckets in
/// table order, each in insertion order (a resize splits a bucket keeping
/// that order). Java turns a bucket of more than 8 keys into a tree in a
/// table of 64 or more buckets, whose order this does not follow: such an
/// insertion is an error.
pub(crate) struct JavaHashMap<K> {
    table: Vec<Vec<usize>>,
    entries: Vec<(i32, K)>,
}

impl<K: PartialEq> JavaHashMap<K> {
    pub(crate) fn new() -> Self {
        JavaHashMap {
            table: Vec::new(),
            entries: Vec::new(),
        }
    }

    fn spread(h: i32) -> usize {
        (h ^ ((h as u32) >> 16) as i32) as u32 as usize
    }

    /// The index of `key` (hashing to `hash`), inserting it if absent.
    pub(crate) fn insert(&mut self, key: K, hash: i32) -> Result<usize, String> {
        if self.table.is_empty() {
            self.table = vec![Vec::new(); 16];
        }
        let n = self.table.len();
        let b = Self::spread(hash) & (n - 1);
        if let Some(&i) = self.table[b]
            .iter()
            .find(|&&i| self.entries[i].0 == hash && self.entries[i].1 == key)
        {
            return Ok(i);
        }
        let i = self.entries.len();
        self.entries.push((hash, key));
        self.table[b].push(i);
        if self.table[b].len() > 8 {
            if n < 64 {
                self.resize();
            } else {
                return Err("more than 8 names fall in the same hash bucket, and the order of the entries cannot \
                            be determined".into());
            }
        }
        // `++size > threshold` (0.75 of the table)
        if self.entries.len() > self.table.len() * 3 / 4 {
            self.resize();
        }
        Ok(i)
    }

    fn resize(&mut self) {
        let n = self.table.len() * 2;
        let mut table = vec![Vec::new(); n];
        for bucket in &self.table {
            for &i in bucket {
                table[Self::spread(self.entries[i].0) & (n - 1)].push(i);
            }
        }
        self.table = table;
    }

    /// The indices of the keys in iteration order.
    pub(crate) fn order(&self) -> impl Iterator<Item = usize> + '_ {
        self.table.iter().flatten().copied()
    }

    /// The key of an index.
    pub(crate) fn key(&self, i: usize) -> &K {
        &self.entries[i].1
    }
}

/// `java.util.Objects.hash(Integer seed, Double a, Double b)`.
pub fn objects_hash_int_double_double(seed: i32, a: f64, b: f64) -> i32 {
    objects_hash(&[seed, double_hash(a), double_hash(b)])
}

/// Trove 3 `PrimeFinder.primeCapacities` up to ~47 million, sorted.
///
/// From Colt: Copyright (c) 1999 CERN - European Organization for Nuclear
/// Research. Permission to use, copy, modify, distribute and sell this
/// software and its documentation for any purpose is hereby granted without
/// fee, provided that the above copyright notice appear in all copies and
/// that both that copyright notice and this permission notice appear in
/// supporting documentation. CERN makes no representations about the
/// suitability of this software for any purpose. It is provided "as is"
/// without expressed or implied warranty.
const PRIMES: [i32; 190] = [
    3, 5, 7, 11, 17, 23, 31, 37, 43, 47, 67, 79, 89, 97, 137, 163, 179, 197, 277, 311, 331, 359,
    379, 397, 433, 557, 599, 631, 673, 719, 761, 797, 877, 953, 1039, 1117, 1201, 1277, 1361, 1439,
    1523, 1597, 1759, 1907, 2081, 2237, 2411, 2557, 2729, 2879, 3049, 3203, 3527, 3821, 4177, 4481,
    4831, 5119, 5471, 5779, 6101, 6421, 7057, 7643, 8363, 8963, 9677, 10243, 10949, 11579, 12203,
    12853, 14143, 15287, 16729, 17929, 19373, 20507, 21911, 23159, 24407, 25717, 28289, 30577,
    33461, 35863, 38747, 41017, 43853, 46327, 48817, 51437, 56591, 61169, 66923, 71741, 77509,
    82037, 87719, 92657, 97649, 102877, 113189, 122347, 133853, 143483, 155027, 164089, 175447,
    185323, 195311, 205759, 226379, 244703, 267713, 286973, 310081, 328213, 350899, 370661, 390647,
    411527, 452759, 489407, 535481, 573953, 620171, 656429, 701819, 741337, 781301, 823117, 905551,
    978821, 1070981, 1147921, 1240361, 1312867, 1403641, 1482707, 1562611, 1646237, 1811107,
    1957651, 2141977, 2295859, 2480729, 2625761, 2807303, 2965421, 3125257, 3292489, 3622219,
    3915341, 4283963, 4591721, 4961459, 5251529, 5614657, 5930887, 6250537, 6584983, 7244441,
    7830701, 8567929, 9183457, 9922933, 10503061, 11229331, 11861791, 12501169, 13169977, 14488931,
    15661423, 17135863, 18366923, 19845871, 21006137, 22458671, 23723597, 25002389, 26339969,
    28977863, 31322867, 34271747, 36733847, 39691759, 42012281, 44917381, 47447201,
];

/// `PrimeFinder.nextPrime`: the smallest table prime >= `n`.
fn next_prime(n: i32) -> i32 {
    let i = PRIMES.partition_point(|&p| p < n);
    *PRIMES
        .get(i)
        .expect("hash table capacity beyond the prime table")
}

/// Key layout of a Trove 3 `TIntIntHashMap` / `TIntObjectHashMap` with no
/// removals, enough to reproduce its iteration order.
pub struct TroveIntKeys {
    slots: Vec<Option<i32>>,
    size: usize,
    free: usize,
    max_size: usize,
    load_factor: f32,
}

impl TroveIntKeys {
    /// `new TIntIntHashMap()`: capacity for 10 entries at load factor 0.5.
    pub fn new() -> Self {
        Self::with_capacity(10, 0.5)
    }

    /// `new TIntIntHashMap(initialCapacity, loadFactor)`.
    pub fn with_capacity(initial: usize, load_factor: f32) -> Self {
        let cap = next_prime((initial as f32 / load_factor).ceil() as i32) as usize;
        let mut t = TroveIntKeys {
            slots: vec![None; cap],
            size: 0,
            free: cap,
            max_size: 0,
            load_factor,
        };
        t.compute_max_size();
        t
    }

    fn compute_max_size(&mut self) {
        let cap = self.slots.len();
        self.max_size = (cap - 1).min((cap as f32 * self.load_factor) as usize);
        self.free = cap - self.size;
    }

    /// Slot for `key`, or `Err(slot)` if it is already present.
    fn insert_key(&mut self, key: i32) -> Result<usize, usize> {
        let len = self.slots.len() as i32;
        let hash = key & 0x7fff_ffff;
        let mut index = hash % len;
        match self.slots[index as usize] {
            None => {
                self.slots[index as usize] = Some(key);
                return Ok(index as usize);
            }
            Some(k) if k == key => return Err(index as usize),
            _ => {}
        }
        let probe = 1 + hash % (len - 2);
        let start = index;
        loop {
            index -= probe;
            if index < 0 {
                index += len;
            }
            match self.slots[index as usize] {
                None => {
                    self.slots[index as usize] = Some(key);
                    return Ok(index as usize);
                }
                Some(k) if k == key => return Err(index as usize),
                _ => {}
            }
            assert!(index != start, "hash table full");
        }
    }

    /// `put(key, ...)`: insert if absent.
    pub fn insert(&mut self, key: i32) {
        if self.insert_key(key).is_err() {
            return;
        }
        self.free -= 1;
        self.size += 1;
        if self.size > self.max_size || self.free == 0 {
            let cap = self.slots.len();
            let new_cap = if self.size > self.max_size {
                next_prime((cap as i32) << 1) as usize
            } else {
                cap
            };
            let old = std::mem::replace(&mut self.slots, vec![None; new_cap]);
            for k in old.into_iter().rev().flatten() {
                let _ = self.insert_key(k);
            }
            self.compute_max_size();
        }
    }

    /// Keys in iteration order (`forEachEntry`: highest slot first).
    pub fn iteration_order(&self) -> impl Iterator<Item = i32> + '_ {
        self.slots.iter().rev().flatten().copied()
    }
}

impl Default for TroveIntKeys {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mersenne_twister_matches_r5_jar() {
        // Values from running commons-math3 3.0 in the R5 7.6 jar.
        let h = objects_hash_int_double_double(42, -30.0346, -51.2177);
        assert_eq!(h, 2_122_154_921);
        let mut mt = MersenneTwister::from_long(i64::from(h));
        assert_eq!(
            [
                mt.next_int(600),
                mt.next_int(600),
                mt.next_int(512),
                mt.next_int(7)
            ],
            [41, 331, 167, 4]
        );
    }

    #[test]
    fn trove_iteration_order_example() {
        // From the transit spec: keys inserted in increasing order.
        let mut t = TroveIntKeys::new();
        for k in [0, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89] {
            t.insert(k);
        }
        let order: Vec<i32> = t.iteration_order().collect();
        assert_eq!(order, [21, 89, 13, 34, 55, 8, 5, 3, 2, 1, 0]);
    }
}

#[cfg(test)]
mod trove_vs_java {
    use super::TroveIntKeys;

    /// Java `String.hashCode` of the comma-joined iteration order, as printed
    /// by a harness inserting the same keys into Trove 3 `TIntIntHashMap`.
    fn java_string_hash(s: &str) -> i32 {
        s.encode_utf16()
            .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)))
    }

    #[test]
    fn iteration_order_matches_trove_through_rehashes() {
        let expected = [
            (5, JAVA[0]),
            (11, JAVA[1]),
            (12, JAVA[2]),
            (30, JAVA[3]),
            (500, JAVA[4]),
            (5000, JAVA[5]),
            (20000, JAVA[6]),
        ];
        for (n, want) in expected {
            let mut t = TroveIntKeys::new();
            for i in 0..n as i64 {
                t.insert(((i * 7919 + 13) % 100_003) as i32);
            }
            let s: String = t.iteration_order().map(|k| format!("{k},")).collect();
            assert_eq!(java_string_hash(&s), want, "n = {n}");
        }
    }

    const JAVA: [i32; 7] = [
        880602725, -16764786, 329292314, 1725429492, 1216150493, 614932100, -290355797,
    ];
}
