//! Per-device Lamport version tracking for the row-sync protocol (P1.3,
//! P2P-003 deep fix).
//!
//! Wall-clock `modified_at` timestamps are attacker-influenced, so LWW on
//! them lets a forged far-future timestamp win forever (the 24h skew bound
//! only caps the damage). Instead, every row write is identified by a
//! **version stamp** `(counter, device_id)`:
//!
//! - `counter` comes from a per-device monotonic Lamport clock. It bumps
//!   when this device sends a row it has locally edited, and merges
//!   (`max(own, received)`) when a remote row is applied — so a genuinely
//!   later write always gets a strictly higher counter than any write it
//!   has seen.
//! - `device_id` breaks ties deterministically for concurrent writes.
//!
//! The store also remembers, per row, the **content hash** the version was
//! stamped over. Local edits don't go through the sync layer, so they're
//! stamped lazily: when a row's current hash no longer matches the recorded
//! one, the row implicitly has a fresh local write and gets the next
//! counter. Replayed or rolled-back rows fail the `incoming > known`
//! comparison and are rejected.
//!
//! Persistence is a single JSON file written through
//! [`sovereign_crypto::fs_private::write_private`] (atomic, owner-only).
//! The content is non-secret metadata (counters + SHA-256 hashes), so it is
//! not encrypted. A `None` path makes the store ephemeral (tests).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{P2pError, P2pResult};

/// The version stamp of a row write: who wrote it and at which Lamport
/// counter, plus the content hash the stamp covers (used to detect local
/// edits made since).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RowVersion {
    pub counter: u64,
    pub device_id: String,
    pub content_hash: String,
}

impl RowVersion {
    /// Total order on version stamps: counter first, then device_id as a
    /// deterministic tiebreak for concurrent writes.
    pub fn ordering_key(&self) -> (u64, &str) {
        (self.counter, self.device_id.as_str())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoreData {
    /// This device's Lamport clock. Strictly increases on every local
    /// stamp; merged to `max(own, received)` on apply.
    own_counter: u64,
    /// Last known version per row id (rows ids are SurrealDB Things,
    /// e.g. `thread:abc`, so they're already table-qualified).
    rows: HashMap<String, RowVersion>,
}

/// Persistent per-device Lamport clock + per-row version map.
#[derive(Debug)]
pub struct VersionStore {
    data: StoreData,
    /// `None` → ephemeral (unit tests); `Some` → saved on every mutation
    /// batch via `save()`.
    path: Option<PathBuf>,
}

impl VersionStore {
    /// An ephemeral store that never touches disk.
    pub fn ephemeral() -> Self {
        Self {
            data: StoreData::default(),
            path: None,
        }
    }

    /// Load from `path`, or start fresh if the file is missing/invalid.
    /// A corrupt store is safe to reset: counters re-merge upward from
    /// whatever peers send, and rows just get re-stamped.
    pub fn load_or_default(path: PathBuf) -> Self {
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|json| serde_json::from_str::<StoreData>(&json).ok())
            .unwrap_or_default();
        Self {
            data,
            path: Some(path),
        }
    }

    /// Persist to disk (atomic, owner-only). No-op for ephemeral stores.
    pub fn save(&self) -> P2pResult<()> {
        let Some(ref path) = self.path else {
            return Ok(());
        };
        let json = serde_json::to_string(&self.data)
            .map_err(|e| P2pError::SyncError(format!("version store serialize: {e}")))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| P2pError::SyncError(format!("version store dir: {e}")))?;
        }
        sovereign_crypto::fs_private::write_private(path, json)
            .map_err(|e| P2pError::SyncError(format!("version store write: {e}")))?;
        Ok(())
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The current version of a local row, stamping a fresh one if the row
    /// was edited locally since the last stamp (its hash diverged from the
    /// recorded one) or was never stamped at all.
    pub fn current_version(
        &mut self,
        row_id: &str,
        content_hash: &str,
        own_device_id: &str,
    ) -> RowVersion {
        if let Some(v) = self.data.rows.get(row_id) {
            if v.content_hash == content_hash {
                return v.clone();
            }
        }
        // Local write not yet stamped → mint the next counter.
        self.data.own_counter += 1;
        let v = RowVersion {
            counter: self.data.own_counter,
            device_id: own_device_id.to_string(),
            content_hash: content_hash.to_string(),
        };
        self.data.rows.insert(row_id.to_string(), v.clone());
        v
    }

    /// Record that a remote version was applied to a row, and merge the
    /// Lamport clock so our future writes order after everything we've
    /// seen (`own = max(own, received)`).
    pub fn record_applied(&mut self, row_id: &str, version: RowVersion) {
        if version.counter > self.data.own_counter {
            self.data.own_counter = version.counter;
        }
        self.data.rows.insert(row_id.to_string(), version);
    }

    /// Drop a row's version entry (e.g. when the row is purged locally).
    pub fn forget_row(&mut self, row_id: &str) {
        self.data.rows.remove(row_id);
    }

    /// The device's current Lamport counter (test/diagnostic).
    pub fn own_counter(&self) -> u64 {
        self.data.own_counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_row_keeps_its_version() {
        let mut vs = VersionStore::ephemeral();
        let v1 = vs.current_version("thread:1", "hashA", "dev-1");
        let v2 = vs.current_version("thread:1", "hashA", "dev-1");
        assert_eq!(v1, v2, "same content must not mint a new counter");
        assert_eq!(vs.own_counter(), 1);
    }

    #[test]
    fn local_edit_mints_higher_counter() {
        let mut vs = VersionStore::ephemeral();
        let v1 = vs.current_version("thread:1", "hashA", "dev-1");
        let v2 = vs.current_version("thread:1", "hashB", "dev-1");
        assert!(v2.counter > v1.counter, "edited row must get a fresh counter");
    }

    #[test]
    fn lamport_merge_on_apply() {
        let mut vs = VersionStore::ephemeral();
        vs.current_version("thread:1", "hashA", "dev-1"); // own = 1
        vs.record_applied(
            "thread:2",
            RowVersion {
                counter: 41,
                device_id: "dev-2".into(),
                content_hash: "hashR".into(),
            },
        );
        // Next local write must order AFTER the applied remote write.
        let v = vs.current_version("thread:3", "hashC", "dev-1");
        assert_eq!(v.counter, 42);
    }

    #[test]
    fn ordering_key_breaks_ties_by_device() {
        let a = RowVersion { counter: 5, device_id: "dev-a".into(), content_hash: "h".into() };
        let b = RowVersion { counter: 5, device_id: "dev-b".into(), content_hash: "h".into() };
        assert!(b.ordering_key() > a.ordering_key());
        let c = RowVersion { counter: 6, device_id: "dev-a".into(), content_hash: "h".into() };
        assert!(c.ordering_key() > b.ordering_key());
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = std::env::temp_dir().join("sovereign-p2p-test-version-store");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("sync_versions.json");

        let mut vs = VersionStore::load_or_default(path.clone());
        vs.current_version("thread:1", "hashA", "dev-1");
        vs.record_applied(
            "entity:9",
            RowVersion { counter: 7, device_id: "dev-2".into(), content_hash: "hB".into() },
        );
        vs.save().unwrap();

        let mut vs2 = VersionStore::load_or_default(path);
        assert_eq!(vs2.own_counter(), 7, "merged counter must persist");
        let v = vs2.current_version("thread:1", "hashA", "dev-1");
        assert_eq!(v.counter, 1, "stored stamp must survive reload");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_store_resets_clean() {
        let dir = std::env::temp_dir().join("sovereign-p2p-test-version-store-corrupt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sync_versions.json");
        std::fs::write(&path, "{not json").unwrap();

        let vs = VersionStore::load_or_default(path);
        assert_eq!(vs.own_counter(), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
