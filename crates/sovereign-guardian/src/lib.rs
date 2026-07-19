//! # sovereign-guardian — the minimal guardian app (G1)
//!
//! The app a friend installs to hold ONE piece of your recovery: a Shamir
//! key-shard. 3 of 5 such shards reconstruct the Recovery Key; fewer reveal
//! nothing. The guardian never holds content (ciphertext fragments live
//! on the owner's own devices and, later, the mesh), never syncs, never
//! pairs — its entire job is:
//!
//! - **enroll** (in-person QR + spoken code — see
//!   `sovereign_p2p::guardian_enroll`),
//! - **custody**: keep shards, possibly for several people,
//! - **heartbeat**: prove possession when the owner pings (G3),
//! - **recovery**: approve or deny shard-release requests inside the 72h
//!   anti-coercion window (the delay is a fraud window a thief can't
//!   skip; call the person first).
//!
//! Dependency rule (locked): `sovereign-p2p` + `sovereign-crypto` only —
//! no db, no ai, no skills, no shell, no comms, no vault. Friends install
//! this; it must stay small and boring.
//!
//! ## At-rest model (G1 scaffold, revisit at mobile packaging)
//! Shards are sealed (XChaCha20-Poly1305) under a per-install random
//! custody key; the custody key and the identity root are written with
//! `fs_private` (0600 / per-user ACL). On the same disk this is
//! defense-in-depth, not a hard boundary — the real hardening for the
//! Android build is the platform keystore, tracked for the Tauri mobile
//! packaging step. The guardian holds one shard of a 3-of-5 split, so a
//! single compromised guardian device reveals nothing by itself.

pub mod custody;
pub mod enroll;
pub mod error;
pub mod serve;
pub mod state;

pub use custody::{CustodyStore, GuardianDuty};
pub use error::{GuardianError, GuardianResult};
pub use state::GuardianIdentity;
