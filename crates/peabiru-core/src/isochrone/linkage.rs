//! The links of a grid's cells to the streets, kept with the network for
//! later surfaces (R5 `LinkageCache` of `LinkedPointSet`s).
//!
//! Linking every cell of a Web Mercator grid is most of the work of a
//! travel-time surface from a few origins (some 160,000 cells for Porto
//! Alegre at zoom 12). A cell's link ([`find_split`](crate::street::link::find_split))
//! depends on the network's edges, the cell's centre and the mode's link
//! flag, and on nothing else: not on speeds, limits or any other routing
//! parameter. R5 keys its linkages the same way (point set, street layer,
//! street mode; the link radii are constants). A grid is fixed by the
//! network's envelope and the zoom, so the key here is the grid itself
//! with the mode. The network's arrays do not change once it can be routed
//! on (see the invariant on [`StreetNetwork`]'s fields), so a kept linkage
//! is never stale; `TransitLayer::link_stops` drops them all the same.

use super::grid::WebMercatorGrid;
use crate::street::link::{link_points, Split};
use crate::street::{Mode, StreetNetwork};
use crate::{CancelToken, RouteError};
use std::sync::{Arc, Mutex, MutexGuard};

/// The links of every cell of a grid, in cell order (`None` for a cell
/// that cannot be linked).
pub(crate) type Links = Arc<Vec<Option<Split>>>;

/// Most linkages kept (R5's `LINKAGE_CACHE_SIZE`).
const MAX_ENTRIES: usize = 9;

/// Bytes the kept linkages may hold in all; the one used last is kept
/// even when it alone is larger.
const MAX_BYTES: usize = 64 << 20;

/// Bytes held by the links of `n` cells.
pub(crate) fn bytes_of(n: usize) -> usize {
    n * std::mem::size_of::<Option<Split>>()
}

/// What a linkage depends on besides the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    grid: WebMercatorGrid,
    mode: Mode,
}

/// The linkages a [`StreetNetwork`] keeps, most recently used first, at
/// most [`MAX_ENTRIES`] and [`MAX_BYTES`]. Clones share the linkages,
/// which never change once built.
#[derive(Default)]
pub(crate) struct Cache(Mutex<Vec<(Key, Links)>>);

impl Cache {
    fn lock(&self) -> MutexGuard<'_, Vec<(Key, Links)>> {
        // nothing panics while holding the lock, but a poisoned list is
        // still a valid one
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The linkage kept for `key`, marked the most recently used.
    fn get(&self, key: Key) -> Option<Links> {
        let mut k = self.lock();
        let i = k.iter().position(|e| e.0 == key)?;
        let e = k.remove(i);
        let links = Arc::clone(&e.1);
        k.insert(0, e);
        Some(links)
    }

    /// Keep `links` for `key` as the most recently used, dropping the
    /// least recently used linkages beyond the bounds.
    fn put(&self, key: Key, links: Links) {
        let mut k = self.lock();
        k.retain(|e| e.0 != key);
        k.insert(0, (key, links));
        let mut total = 0;
        let mut kept = 0;
        k.retain(|e| {
            total += bytes_of(e.1.len());
            kept += 1;
            kept == 1 || (kept <= MAX_ENTRIES && total <= MAX_BYTES)
        });
    }

    #[cfg(test)]
    fn keys(&self) -> Vec<(i32, Mode)> {
        self.lock()
            .iter()
            .map(|e| (e.0.grid.zoom(), e.0.mode))
            .collect()
    }
}

impl Clone for Cache {
    fn clone(&self) -> Self {
        Cache(Mutex::new(self.lock().clone()))
    }
}

impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let k = self.lock();
        let mut l = f.debug_list();
        for (key, links) in k.iter() {
            l.entry(&format_args!(
                "{:?} at zoom {}: {} cells, {} bytes",
                key.mode,
                key.grid.zoom(),
                links.len(),
                bytes_of(links.len())
            ));
        }
        l.finish()
    }
}

/// The links of `grid`'s cells by `mode`: the network's kept ones, or else
/// linked now on up to `n_threads` threads and kept.
/// [`RouteError::Cancelled`] when `cancel` is cancelled while linking.
pub(crate) fn grid_links(
    net: &StreetNetwork,
    grid: &WebMercatorGrid,
    mode: Mode,
    n_threads: usize,
    cancel: &CancelToken,
) -> Result<Links, RouteError> {
    let key = Key { grid: *grid, mode };
    if let Some(links) = net.grid_links.get(key) {
        return Ok(links);
    }
    let links = link_points(
        net,
        grid.n_cells(),
        |i| grid.cell_center(i),
        mode,
        n_threads,
        cancel,
    )
    .ok_or(RouteError::Cancelled)?;
    let links = Arc::new(links);
    net.grid_links.put(key, Arc::clone(&links));
    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::street::link::find_split;
    use crate::BuildOptions;

    fn spo_network(spo: &str) -> StreetNetwork {
        crate::transit::build_from_dir(spo, &BuildOptions::default())
            .unwrap()
            .0
    }

    #[test]
    fn kept_links_are_those_of_each_cell() {
        let Some(spo) = crate::sample_data::spo() else {
            return;
        };
        let net = spo_network(spo);
        let cancel = CancelToken::default();
        let grid = WebMercatorGrid::for_street(&net, 11).unwrap();
        let mut scratch = Vec::new();
        for mode in [Mode::Walk, Mode::Bicycle, Mode::Car] {
            for threads in [1, 3] {
                let links = grid_links(&net, &grid, mode, threads, &cancel).unwrap();
                assert_eq!(links.len(), grid.n_cells());
                for (i, l) in links.iter().enumerate() {
                    let (lat, lon) = grid.cell_center(i);
                    assert_eq!(
                        *l,
                        find_split(&net, lat, lon, mode, &mut scratch),
                        "cell {i}"
                    );
                }
            }
        }
        assert!(net
            .grid_links
            .get(Key {
                grid,
                mode: Mode::Car
            })
            .is_some());
    }

    #[test]
    fn linkages_are_kept_by_grid_and_mode_within_bounds() {
        let Some(spo) = crate::sample_data::spo() else {
            return;
        };
        let net = spo_network(spo);
        let cancel = CancelToken::default();
        let grid = |z| WebMercatorGrid::for_street(&net, z).unwrap();
        let first = grid_links(&net, &grid(10), Mode::Walk, 2, &cancel).unwrap();
        let again = grid_links(&net, &grid(10), Mode::Walk, 2, &cancel).unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        // a clone shares what is kept, and keeps its own list afterwards
        let copy = net.clone();
        assert!(Arc::ptr_eq(
            &first,
            &grid_links(&copy, &grid(10), Mode::Walk, 2, &cancel).unwrap()
        ));
        grid_links(&copy, &grid(9), Mode::Walk, 2, &cancel).unwrap();
        assert_eq!(net.grid_links.keys(), vec![(10, Mode::Walk)]);
        // another mode at the same zoom is another linkage
        let car = grid_links(&net, &grid(10), Mode::Car, 2, &cancel).unwrap();
        assert!(!Arc::ptr_eq(&first, &car));
        // more keys than are kept: the least recently used go first
        for z in 9..=12 {
            for mode in [Mode::Walk, Mode::Bicycle, Mode::Car] {
                grid_links(&net, &grid(z), mode, 2, &cancel).unwrap();
            }
        }
        let keys = net.grid_links.keys();
        assert_eq!(keys.len(), MAX_ENTRIES);
        assert_eq!(keys[0], (12, Mode::Car));
        assert!(!keys.contains(&(9, Mode::Walk)));
        // the one used last moves to the front
        grid_links(&net, &grid(11), Mode::Walk, 2, &cancel).unwrap();
        assert_eq!(net.grid_links.keys()[0], (11, Mode::Walk));
        // dropped with the hierarchies when the network changes
        let mut net = net;
        net.forget_kept();
        assert!(net.grid_links.keys().is_empty());
    }

    #[test]
    fn the_byte_bound_keeps_the_last_linkage_used() {
        let cache = Cache::default();
        let grid = |w| WebMercatorGrid::new(0, 0, w, 1000, 12).unwrap();
        let links = |n| Arc::new(vec![None; n]);
        // as documented, on 64-bit and WebAssembly targets alike
        assert_eq!(bytes_of(1), 40);
        let big = MAX_BYTES / bytes_of(1) / 2 + 1; // two do not fit
        cache.put(
            Key {
                grid: grid(1),
                mode: Mode::Walk,
            },
            links(1000),
        );
        cache.put(
            Key {
                grid: grid(2),
                mode: Mode::Walk,
            },
            links(big),
        );
        assert_eq!(cache.lock().len(), 2);
        cache.put(
            Key {
                grid: grid(3),
                mode: Mode::Walk,
            },
            links(big),
        );
        let widths: Vec<i32> = cache.lock().iter().map(|e| e.0.grid.width()).collect();
        assert_eq!(widths, vec![3]);
        // one larger than the bound alone is still kept
        cache.put(
            Key {
                grid: grid(4),
                mode: Mode::Car,
            },
            links(2 * big),
        );
        assert_eq!(cache.lock().len(), 1);
    }

    #[test]
    fn a_cancelled_linkage_is_not_kept() {
        let Some(spo) = crate::sample_data::spo() else {
            return;
        };
        let net = spo_network(spo);
        let cancel = CancelToken::default();
        cancel.cancel();
        let grid = WebMercatorGrid::for_street(&net, 12).unwrap();
        assert!(matches!(
            grid_links(&net, &grid, Mode::Walk, 2, &cancel),
            Err(RouteError::Cancelled)
        ));
        assert!(net.grid_links.keys().is_empty());
    }
}
