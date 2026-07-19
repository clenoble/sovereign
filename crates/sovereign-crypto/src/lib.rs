//! Sovereign's cryptographic core: the key hierarchy (Master -> Device -> KEK
//! -> document keys), AEAD, the auth store, the PII vault, and Guardian Access
//! Recovery.
//!
//! # Invariant: this crate has NO logging surface. Keep it that way.
//!
//! There is deliberately **no `tracing` dependency and not one `tracing::`
//! call** anywhere in this crate — including `auth`, `kek`, `master_key`,
//! `key_db`, `vault` and `recovery_store`. That is not an oversight; it is the
//! same "hard barriers over trust" shape as the `Plane` enum: code that cannot
//! log cannot accidentally log key material. A prompt or a review can be
//! forgotten — a missing dependency cannot.
//!
//! It is load-bearing in practice, not in theory. During the F1 live run an
//! attempt to add a temporary env-gated log of a guardian **enrollment code**
//! (the secret gating the shard handoff) was caught and refused. A crate with
//! no logging surface makes that class of mistake structurally impossible
//! rather than dependent on someone noticing.
//!
//! **So: do not `cargo add tracing` here, and do not reach for a log to debug
//! this crate.** Return the information instead and let the caller decide what
//! to say about it — see [`recovery_store::Reconciled`], which exists for
//! exactly this reason. Callers (`sovereign-app`, `sovereign-shell`) have
//! `tracing` and are the right place for it.
//!
//! *(Written down 2026-07-16: the property was real, consistent across ~22
//! modules, and documented nowhere. An undocumented load-bearing invariant is
//! how the KEK/DeviceKey drift happened — the spec said one thing, the code
//! did another, and nothing diffed them.)*

pub use ed25519_dalek;
pub use zeroize;

pub mod account_key;
pub mod backup_signing;
pub mod aead;
pub mod auth;
pub mod canary;
pub mod device_key;
pub mod pair_payload;
pub mod document_key;
pub mod error;
pub mod fs_private;
pub mod index_key;
pub mod kek;
pub mod key_db;
pub mod keystroke;
pub mod mac;
pub mod master_key;
pub mod password_gen;
pub mod recovery_key;
pub mod recovery_roster;
pub mod recovery_seal;
pub mod recovery_store;
pub mod vault;

pub mod migration;

#[cfg(feature = "guardian")]
pub mod guardian;

pub use error::{CryptoError, CryptoResult};

/// A fresh cryptographically-random 32-byte value as lowercase hex (64 chars).
/// Used for shared secrets such as the loopback sidecar token (SIDECAR-002).
pub fn random_hex_32() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
