//! What the core asks of the platform: worker threads and a clock.
//!
//! WebAssembly without thread support (`wasm32-unknown-unknown`, as used
//! by the JavaScript package) can neither start a thread nor read the
//! clock through `std`: both panic there. Every use goes through this
//! module, which runs the workers one after another on the calling thread
//! on such targets. Results never depend on the number of workers, so they
//! are the same either way.

/// Whether threads can be started on this target.
const CAN_SPAWN: bool = !cfg!(target_family = "wasm");

/// Cores available to the worker threads: 1 where no thread can be
/// started.
pub(crate) fn cores() -> usize {
    if !CAN_SPAWN {
        return 1;
    }
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// `work(state)` for every state, each on a scoped thread of its own, and
/// the results in the order of `states`. A worker's panic is raised again
/// on the calling thread once every worker has ended. Where no thread can
/// be started, the workers run one after another on the calling thread
/// (callers ask for one worker there: see [`cores`]).
pub(crate) fn scoped<S: Send, T: Send>(
    states: impl IntoIterator<Item = S>,
    work: impl Fn(S) -> T + Sync,
) -> Vec<T> {
    if !CAN_SPAWN {
        return states.into_iter().map(work).collect();
    }
    let work = &work;
    std::thread::scope(|scope| {
        let handles: Vec<_> = states
            .into_iter()
            .map(|s| scope.spawn(move || work(s)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_else(|p| std::panic::resume_unwind(p)))
            .collect()
    })
}

/// A seed that differs from call to call, for Monte Carlo draws without a
/// seed: the clock in nanoseconds, as R5 seeds them. Where `std` has no
/// clock (WebAssembly), a counter mixed with a constant, which differs
/// from call to call within a process but repeats from one process to the
/// next; the JavaScript package draws a seed of its own instead.
pub(crate) fn clock_seed() -> i64 {
    if cfg!(target_family = "wasm") {
        use std::sync::atomic::{AtomicU64, Ordering};
        static CALLS: AtomicU64 = AtomicU64::new(0);
        // splitmix64 of the call count
        let mut z = CALLS
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        return (z ^ (z >> 31)) as i64;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workers_keep_the_order_of_their_states() {
        let mut slots = vec![0u32; 4];
        let out = scoped(slots.iter_mut().enumerate(), |(i, s)| {
            *s = i as u32 * 10;
            i * 2
        });
        assert_eq!(out, [0, 2, 4, 6]);
        assert_eq!(slots, [0, 10, 20, 30]);
        assert!(cores() >= 1);
    }

    #[test]
    #[should_panic(expected = "worker 1")]
    fn a_worker_panic_is_raised_again() {
        scoped(0..3, |i| {
            if i == 1 {
                panic!("worker 1");
            }
        });
    }
}
