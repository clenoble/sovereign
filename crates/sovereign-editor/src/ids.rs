//! Random numbers for block ids and clipboard nonces, from the standard
//! library only: `RandomState` is seeded from the operating system's random
//! source once per thread and changes on every construction. No file, no
//! network, nothing kept.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn random_u64() -> u64 {
    let mut h = RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.finish()
}

/// 128 random bits as 32 hex digits.
pub(crate) fn random_hex128() -> String {
    format!("{:016x}{:016x}", random_u64(), random_u64())
}
