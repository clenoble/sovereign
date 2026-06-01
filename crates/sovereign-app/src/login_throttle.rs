//! Server-side login lockout (audit fix CRYPTO-002).
//!
//! The login command reads `max_login_attempts` / `lockout_seconds` from
//! config, but historically only forwarded them to the UI as DTO fields — the
//! Rust side kept no counter and imposed no delay, so scripted IPC (or direct
//! `authenticate()` calls) could guess unlimited passwords with no throttle,
//! amplifying the at-rest brute-force surface. This module persists a small
//! attempt tracker and lets `validate_password` enforce the lockout
//! server-side.
//!
//! Threat model: the tracker is a *plaintext* JSON file. It defends against
//! scripted / online password guessing via the IPC surface. It does NOT defend
//! against an attacker with filesystem access — such an attacker already holds
//! `auth.store` and can attack it directly (covered by the Argon2id at-rest
//! fix CRYPTO-001). Filesystem tampering of this counter is therefore out of
//! scope: clearing it only resets the online throttle, it does not weaken the
//! at-rest KDF. We treat a missing / unreadable file as "no failures yet".

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Filename for the persisted attempt tracker, stored alongside `auth.store`
/// in the crypto dir.
const TRACKER_FILE: &str = "login_attempts.json";

/// Persisted login-attempt state. `window_start_ms` is the wall-clock time
/// (Unix epoch millis) of the first failure in the current window; the window
/// is considered open from `window_start_ms` for `lockout_seconds`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoginAttempts {
    /// Failed attempts recorded in the current window.
    pub failed_count: u32,
    /// Epoch-millis timestamp of the first failure in the current window.
    pub window_start_ms: i64,
}

/// Resolve the tracker path inside the given crypto dir.
fn tracker_path(crypto_dir: &Path) -> PathBuf {
    crypto_dir.join(TRACKER_FILE)
}

/// Current wall-clock time in epoch millis. Indirected so tests can reason
/// about it via the explicit-time helpers below.
fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl LoginAttempts {
    /// Load the tracker from `crypto_dir/login_attempts.json`. A missing or
    /// unreadable / malformed file is treated as a fresh tracker (no failures)
    /// — see the threat-model note: FS tamper is out of scope.
    pub fn load(crypto_dir: &Path) -> Self {
        let path = tracker_path(crypto_dir);
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist the tracker to `crypto_dir/login_attempts.json`. Best-effort:
    /// returns the IO/serde error stringified so callers can log it without
    /// failing the login flow on a write hiccup.
    pub fn save(&self, crypto_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(crypto_dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(tracker_path(crypto_dir), json).map_err(|e| e.to_string())
    }

    /// Record one failed attempt. The first failure of a fresh window stamps
    /// `window_start_ms`; subsequent failures only bump the counter (the window
    /// keeps sliding from the first failure).
    pub fn record_failure(&mut self) {
        self.record_failure_at(now_ms());
    }

    /// `record_failure` with an explicit clock (for tests).
    pub fn record_failure_at(&mut self, now: i64) {
        if self.failed_count == 0 {
            self.window_start_ms = now;
        }
        self.failed_count = self.failed_count.saturating_add(1);
    }

    /// Reset the tracker to a clean state (called on any successful login —
    /// primary OR duress).
    pub fn reset(&mut self) {
        self.failed_count = 0;
        self.window_start_ms = 0;
    }

    /// If currently locked, return `Some(remaining_secs)` (>= 1); otherwise
    /// `None`. The account is locked when `failed_count >= max` AND the lockout
    /// window has not yet elapsed. Once the window fully elapses the caller
    /// should `reset()` and treat the tracker as fresh.
    ///
    /// `max == 0` disables the lockout entirely (never locks).
    pub fn is_locked(&self, max: u32, lockout_secs: u32) -> Option<u32> {
        self.is_locked_at(max, lockout_secs, now_ms())
    }

    /// `is_locked` with an explicit clock (for tests).
    pub fn is_locked_at(&self, max: u32, lockout_secs: u32, now: i64) -> Option<u32> {
        if max == 0 || self.failed_count < max {
            return None;
        }
        let lockout_ms = i64::from(lockout_secs) * 1000;
        let elapsed_ms = now.saturating_sub(self.window_start_ms);
        if elapsed_ms >= lockout_ms {
            // Window fully elapsed — no longer locked.
            return None;
        }
        let remaining_ms = lockout_ms - elapsed_ms;
        // Round up to at least 1 second so the message never says "0 seconds".
        Some(((remaining_ms + 999) / 1000) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: u32 = 10;
    const LOCKOUT: u32 = 300; // 5 minutes
    const T0: i64 = 1_700_000_000_000; // arbitrary fixed epoch-millis

    #[test]
    fn record_failure_increments_and_stamps_window() {
        let mut a = LoginAttempts::default();
        assert_eq!(a.failed_count, 0);
        assert_eq!(a.window_start_ms, 0);

        a.record_failure_at(T0);
        assert_eq!(a.failed_count, 1);
        assert_eq!(a.window_start_ms, T0, "first failure stamps the window");

        // Later failures bump the count but keep the original window start.
        a.record_failure_at(T0 + 5_000);
        assert_eq!(a.failed_count, 2);
        assert_eq!(a.window_start_ms, T0, "window start does not move on later failures");
    }

    #[test]
    fn locks_once_failed_count_reaches_max_within_window() {
        let mut a = LoginAttempts::default();
        // 9 failures — not yet locked.
        for i in 0..(MAX - 1) {
            a.record_failure_at(T0 + i64::from(i));
        }
        assert_eq!(a.failed_count, MAX - 1);
        assert_eq!(a.is_locked_at(MAX, LOCKOUT, T0 + 1_000), None, "below max is not locked");

        // 10th failure within the window — now locked.
        a.record_failure_at(T0 + 100);
        assert_eq!(a.failed_count, MAX);
        let remaining = a.is_locked_at(MAX, LOCKOUT, T0 + 1_000);
        assert!(remaining.is_some(), "at max within window is locked");
        let secs = remaining.unwrap();
        assert!(secs <= LOCKOUT && secs >= LOCKOUT - 1, "remaining ~= lockout window, got {secs}");
    }

    #[test]
    fn reset_clears_the_tracker() {
        let mut a = LoginAttempts::default();
        for i in 0..MAX {
            a.record_failure_at(T0 + i64::from(i));
        }
        assert!(a.is_locked_at(MAX, LOCKOUT, T0 + 1_000).is_some());

        a.reset();
        assert_eq!(a.failed_count, 0);
        assert_eq!(a.window_start_ms, 0);
        assert_eq!(a.is_locked_at(MAX, LOCKOUT, T0 + 1_000), None, "reset clears the lock");
    }

    #[test]
    fn failure_after_window_fully_elapsed_is_not_locked() {
        let mut a = LoginAttempts::default();
        for i in 0..MAX {
            a.record_failure_at(T0 + i64::from(i));
        }
        // At max within the window → locked.
        assert!(a.is_locked_at(MAX, LOCKOUT, T0 + 1_000).is_some());

        // Once the lockout window fully elapses, is_locked reports unlocked
        // so the caller resets and a fresh window can start.
        let after = T0 + i64::from(LOCKOUT) * 1000 + 1;
        assert_eq!(
            a.is_locked_at(MAX, LOCKOUT, after),
            None,
            "after the window fully elapses the account is no longer locked"
        );

        // Caller resets, then a single new failure starts a fresh window and
        // is well below max — not locked.
        a.reset();
        a.record_failure_at(after);
        assert_eq!(a.failed_count, 1);
        assert_eq!(a.window_start_ms, after, "fresh window starts at the new failure");
        assert_eq!(
            a.is_locked_at(MAX, LOCKOUT, after + 1_000),
            None,
            "a single failure in a fresh window is not locked"
        );
    }

    #[test]
    fn max_zero_disables_lockout() {
        let mut a = LoginAttempts::default();
        for i in 0..50 {
            a.record_failure_at(T0 + i64::from(i));
        }
        assert_eq!(a.is_locked_at(0, LOCKOUT, T0 + 1_000), None, "max==0 never locks");
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = LoginAttempts::default();
        a.record_failure_at(T0);
        a.record_failure_at(T0 + 10);
        a.save(dir.path()).unwrap();

        let loaded = LoginAttempts::load(dir.path());
        assert_eq!(loaded.failed_count, 2);
        assert_eq!(loaded.window_start_ms, T0);
    }

    #[test]
    fn missing_file_loads_as_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let a = LoginAttempts::load(dir.path());
        assert_eq!(a.failed_count, 0);
        assert_eq!(a.window_start_ms, 0);
    }
}
