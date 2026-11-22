//! The application's clock: the « now » every reader of persona data goes
//! through — the document service, its trash and purge (`docs_host`
//! builds the service's `Clock` over it), and any other part of the app that
//! dates or ages a persona's data. Wall-clock time.
//!
//! Under the `test-support` feature (a host's or a crate's
//! `[dev-dependencies]` only, never a release build), [`test_clock`] moves
//! it, forward or backward, for the whole process: C-AREA AR4's purge after a
//! switch, and L6's « the trash's 30 days guarded against a clock jump »
//! (v0.0.12 review, finding 8).
//!
//! **The rule for tests** (claude-coord from-fable-2nde-position/0027 §2):
//! the clock is one per process, and cargo runs a binary's tests on several
//! threads at once.
//! - A test that moves the clock holds a [`test_clock::ClockGuard`]
//!   (`test_clock::hold()`) for as long as it needs the moved clock, or runs
//!   alone in its own test binary or child process. The clock cannot be moved
//!   without a guard: `set` and `advance` are the guard's.
//! - A test that only reads « now », in a binary where another test moves
//!   the clock, holds a guard too: holding it is what makes it read the wall
//!   clock.
//! - Dropping the guard, on panic too, puts the clock back on the wall clock
//!   and lets the next test take it.
//!
//! **How L6's test uses it:** `let clock = test_clock::hold();` at the start;
//! trash a document and run the unlock tick; `clock.advance(Duration::days(
//! 3650))` for a jump forward, and check the 30-day guard; `clock.advance(-
//! …)` or `clock.set(Some(past))` for a jump backward, and check the days
//! left stay within 0 to 30 and no purge is revived or delayed beyond the
//! rule; the guard's drop ends it.
//!
//! **Types and signatures** (claude-coord from-windows/0249,
//! from-fable-2nde-position/0025 §1; by the test author): moving the app's
//! other readers of « now » onto [`now`] is the implementer's.

use chrono::{DateTime, Utc};

/// The application's « now ».
pub fn now() -> DateTime<Utc> {
    #[cfg(feature = "test-support")]
    {
        return Utc::now() + test_clock::offset();
    }
    #[allow(unreachable_code)]
    Utc::now()
}

/// The test clock: an offset from the wall clock, one per process, so time
/// still flows once it is set. Moved only through a [`ClockGuard`], which
/// holds a process-wide lock. Test wiring only.
#[cfg(feature = "test-support")]
pub mod test_clock {
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::{Mutex, MutexGuard};

    static OFFSET_MS: AtomicI64 = AtomicI64::new(0);
    static LOCK: Mutex<()> = Mutex::new(());

    /// Held by a test for as long as it moves, or relies on, the clock. One
    /// at a time per process; dropping it (on panic too) puts the clock back
    /// on the wall clock.
    pub struct ClockGuard {
        _lock: MutexGuard<'static, ()>,
    }

    /// Takes the clock for this test, on the wall clock. Waits while another
    /// test holds it. A guard left by a test that panicked is taken over.
    pub fn hold() -> ClockGuard {
        let lock = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        OFFSET_MS.store(0, Ordering::SeqCst);
        ClockGuard { _lock: lock }
    }

    impl ClockGuard {
        /// The app's « now » becomes `now` (and flows from there); `None`
        /// puts it back on the wall clock.
        pub fn set(&self, now: Option<chrono::DateTime<chrono::Utc>>) {
            let ms = now.map_or(0, |t| (t - chrono::Utc::now()).num_milliseconds());
            OFFSET_MS.store(ms, Ordering::SeqCst);
        }

        /// Moves the app's « now » by `by`, forward or backward.
        pub fn advance(&self, by: chrono::Duration) {
            OFFSET_MS.fetch_add(by.num_milliseconds(), Ordering::SeqCst);
        }
    }

    impl Drop for ClockGuard {
        fn drop(&mut self) {
            // Runs before the lock (a field) is released.
            OFFSET_MS.store(0, Ordering::SeqCst);
        }
    }

    pub(crate) fn offset() -> chrono::Duration {
        chrono::Duration::milliseconds(OFFSET_MS.load(Ordering::SeqCst))
    }
}
