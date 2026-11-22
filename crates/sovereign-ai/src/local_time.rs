//! Local time as the orchestrator renders it (spec ch. 08, *Action targets*,
//! AT11: dates in local time, in History's format `YYYY-MM-DD HH:MM`).
//!
//! [`offset_at`] is the one function through which the orchestrator gets the
//! local offset: the system's, unless a test has fixed one for its thread
//! ([`fix_offset`], test builds and the `test-support` feature only), so a
//! test can tell local time from UTC on a machine whose offset is zero (CI).
//! The override is per thread: it reaches what runs on the test's own thread
//! (`#[tokio::test]`'s default runtime), not other threads.

use std::cell::Cell;

use chrono::{DateTime, FixedOffset, Local, Utc};

thread_local! {
    static FIXED: Cell<Option<FixedOffset>> = const { Cell::new(None) };
}

/// The local offset at instant `t`: the one a test fixed on this thread, or
/// else the system's.
pub fn offset_at(t: DateTime<Utc>) -> FixedOffset {
    FIXED.with(|f| f.get()).unwrap_or_else(|| *t.with_timezone(&Local).offset())
}

/// `t` in local time ([`offset_at`]), in History's format.
pub fn history_format(t: DateTime<Utc>) -> String {
    t.with_timezone(&offset_at(t)).format("%Y-%m-%d %H:%M").to_string()
}

/// Restores the offset that was in force when it was made.
#[cfg(any(test, feature = "test-support"))]
pub struct FixedOffsetGuard(Option<FixedOffset>);

/// Fixes the local offset of this thread to `offset` until the guard drops.
#[cfg(any(test, feature = "test-support"))]
pub fn fix_offset(offset: FixedOffset) -> FixedOffsetGuard {
    FixedOffsetGuard(FIXED.with(|f| f.replace(Some(offset))))
}

#[cfg(any(test, feature = "test-support"))]
impl Drop for FixedOffsetGuard {
    fn drop(&mut self) {
        FIXED.with(|f| f.set(self.0));
    }
}
