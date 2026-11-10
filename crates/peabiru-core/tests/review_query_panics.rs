//! Public query-time functions on inputs they cannot compute: each returns
//! an error instead of panicking or overflowing. (The same checks of
//! functions internal to the crate are in `src/review_tests.rs`.)

use peabiru_core::arrival_departure;
use std::panic::catch_unwind;

/// `arrival_departure` with month 0 used to underflow in `previous_day` and
/// a huge `max_trip_duration` to overflow the multiplication.
#[test]
fn arrival_departure_does_not_overflow() {
    let ad = |d, t, m| catch_unwind(|| arrival_departure(d, t, m)).expect("panicked");
    assert!(ad(20190001, 0, 120).is_err());
    assert!(ad(0, 0, 120).is_err());
    // i32::MAX minutes (about 4083 years) before 2019 is before year 1
    assert!(ad(20190513, 0, i32::MAX).is_err());
    assert_eq!(ad(20190513, 0, 1_000_000), Ok((20170617, 48000)));
    assert_eq!(ad(20190513, 16 * 3600, 120), Ok((20190513, 14 * 3600)));
}
