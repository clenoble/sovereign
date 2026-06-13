//! P4.2 host side — storing other users' backup fragments and guardian
//! key shards.
//!
//! A host is any opted-in peer. It stores, per owner tag:
//!   - the (public) backup manifest + MasterKey salt,
//!   - opaque ciphertext fragments, up to a per-owner byte quota,
//!   - at most one snapshot per owner: a newer epoch replaces the old.
//!
//! Reciprocity is **accounting, not enforcement** for now: the host
//! tracks bytes hosted per owner (`accounting()`) so the app can show
//! "you host X for them, they host Y for you"; refusal policy is a
//! later pass.
//!
//! Guardian key shards are the protected half of the hybrid model. A
//! shard is released ONLY when (a) this device's user has approved the
//! recovery request AND (b) the release delay (72h by default — the
//! anti-coercion control) has elapsed since the request arrived. Until
//! then `RequestShard` returns no data, and the pending request is
//! surfaced to the user for approval.
//!
//! Everything lives in one JSON state file plus one blob file per
//! fragment, written through `fs_private` (atomic, owner-only).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::backup::sha256_hex;
use crate::error::{P2pError, P2pResult};

/// Default per-owner storage quota for hosted fragments (64 MiB).
pub const DEFAULT_QUOTA_BYTES: u64 = 64 * 1024 * 1024;

/// Default delay between a shard-recovery request arriving and the
/// approved shard being releasable. The anti-coercion control for
/// social recovery — do not lower in production.
pub const DEFAULT_RELEASE_DELAY_HOURS: i64 = 72;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostedFragmentMeta {
    pub index: u8,
    pub digest: String,
    pub len: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostedSnapshot {
    pub snapshot_id: String,
    pub epoch: u32,
    pub stored_at: String,
    pub manifest_json: String,
    pub salt_b64: String,
    pub fragments: Vec<HostedFragmentMeta>,
    pub total_bytes: u64,
}

/// A guardian key shard held for another user, plus the pending
/// recovery-release state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeldGuardianShard {
    pub shard_id: String,
    pub for_user: String,
    pub epoch: u32,
    /// Base64 of the encoded `BackupGuardianPayload` (or legacy raw
    /// share) exactly as delivered.
    pub shard_data: String,
    pub received_at: String,
    /// Set when a recovery request arrives for this user/epoch.
    #[serde(default)]
    pub release_requested_at: Option<String>,
    #[serde(default)]
    pub release_request_id: Option<String>,
    /// Set when THIS device's user approves the release.
    #[serde(default)]
    pub release_approved: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HostState {
    /// owner_tag → hosted snapshot (one per owner; newest epoch wins).
    hosted: HashMap<String, HostedSnapshot>,
    /// Guardian shards held for other users.
    shards: Vec<HeldGuardianShard>,
}

/// Per-owner accounting line for the reciprocity display.
#[derive(Debug, Clone, Serialize)]
pub struct HostedAccount {
    pub owner_tag: String,
    pub snapshot_id: String,
    pub epoch: u32,
    pub fragment_count: usize,
    pub total_bytes: u64,
}

/// Opt-in backup host store. Interior mutability (std Mutex, never held
/// across await) so the node can use it from `&self` contexts.
pub struct BackupHost {
    dir: PathBuf,
    quota_bytes: u64,
    release_delay_hours: i64,
    state: Mutex<HostState>,
}

impl BackupHost {
    /// Open (or initialize) the host store under `dir`.
    pub fn open(dir: PathBuf, quota_bytes: u64) -> Self {
        Self::open_with_delay(dir, quota_bytes, DEFAULT_RELEASE_DELAY_HOURS)
    }

    /// Test hook: a configurable release delay. Production uses
    /// [`DEFAULT_RELEASE_DELAY_HOURS`] via [`Self::open`].
    pub fn open_with_delay(dir: PathBuf, quota_bytes: u64, release_delay_hours: i64) -> Self {
        let state = std::fs::read_to_string(dir.join("backup_host.json"))
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        Self {
            dir,
            quota_bytes,
            release_delay_hours,
            state: Mutex::new(state),
        }
    }

    fn save_state(&self, state: &HostState) -> P2pResult<()> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| P2pError::SyncError(format!("backup host dir: {e}")))?;
        let json = serde_json::to_string(state)
            .map_err(|e| P2pError::SyncError(format!("backup host encode: {e}")))?;
        sovereign_crypto::fs_private::write_private(&self.dir.join("backup_host.json"), json)
            .map_err(|e| P2pError::SyncError(format!("backup host write: {e}")))?;
        Ok(())
    }

    fn fragment_path(&self, owner_tag: &str, snapshot_id: &str, index: u8) -> PathBuf {
        // owner_tag and snapshot_id are hex strings we generated/derived;
        // sanitize anyway since they arrive over the network.
        let safe = |s: &str| -> String {
            s.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
        };
        self.dir
            .join(format!("{}-{}-{index}.frag", safe(owner_tag), safe(snapshot_id)))
    }

    /// Store one fragment. Creates/replaces the owner's hosted snapshot
    /// on first fragment of a new (>=) epoch; rejects older epochs,
    /// digest mismatches, and quota overruns. Returns Ok(accepted).
    #[allow(clippy::too_many_arguments)]
    pub fn store_fragment(
        &self,
        owner_tag: &str,
        snapshot_id: &str,
        epoch: u32,
        manifest_json: &str,
        salt_b64: &str,
        index: u8,
        fragment_bytes: &[u8],
        expected_digest: &str,
    ) -> P2pResult<bool> {
        if sha256_hex(fragment_bytes) != expected_digest {
            tracing::warn!("backup fragment {owner_tag}/{snapshot_id}#{index}: digest mismatch");
            return Ok(false);
        }

        let mut state = self.state.lock().expect("backup host lock poisoned");

        // Epoch policy: one snapshot per owner, newest epoch wins.
        let mut old_files: Vec<PathBuf> = Vec::new();
        match state.hosted.get(owner_tag) {
            Some(existing) if existing.epoch > epoch => {
                tracing::debug!(
                    "rejecting backup fragment for {owner_tag}: epoch {epoch} older than hosted {}",
                    existing.epoch
                );
                return Ok(false);
            }
            Some(existing)
                if existing.epoch < epoch || existing.snapshot_id != snapshot_id =>
            {
                // Replace: collect the old fragment files for cleanup.
                for f in &existing.fragments {
                    old_files.push(self.fragment_path(owner_tag, &existing.snapshot_id, f.index));
                }
                state.hosted.remove(owner_tag);
            }
            _ => {}
        }

        let entry = state
            .hosted
            .entry(owner_tag.to_string())
            .or_insert_with(|| HostedSnapshot {
                snapshot_id: snapshot_id.to_string(),
                epoch,
                stored_at: chrono::Utc::now().to_rfc3339(),
                manifest_json: manifest_json.to_string(),
                salt_b64: salt_b64.to_string(),
                fragments: Vec::new(),
                total_bytes: 0,
            });

        if entry.snapshot_id != snapshot_id {
            // Same epoch, different snapshot id — refuse the ambiguity.
            return Ok(false);
        }
        if entry.fragments.iter().any(|f| f.index == index) {
            return Ok(true); // idempotent re-store
        }
        if entry.total_bytes + fragment_bytes.len() as u64 > self.quota_bytes {
            tracing::warn!(
                "backup fragment for {owner_tag} rejected: quota exceeded ({} + {} > {})",
                entry.total_bytes,
                fragment_bytes.len(),
                self.quota_bytes
            );
            return Ok(false);
        }

        let path = self.fragment_path(owner_tag, snapshot_id, index);
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| P2pError::SyncError(format!("backup host dir: {e}")))?;
        sovereign_crypto::fs_private::write_private(&path, fragment_bytes)
            .map_err(|e| P2pError::SyncError(format!("fragment write: {e}")))?;

        entry.fragments.push(HostedFragmentMeta {
            index,
            digest: expected_digest.to_string(),
            len: fragment_bytes.len() as u64,
        });
        entry.total_bytes += fragment_bytes.len() as u64;

        self.save_state(&state)?;
        drop(state);
        for f in old_files {
            let _ = std::fs::remove_file(f);
        }
        Ok(true)
    }

    /// The hosted (owner_tag, snapshot) pairs matching `owner_tag` (or
    /// all when `None` — fragments are public-safe by design; see
    /// module docs).
    pub fn list_hosted(&self, owner_tag: Option<&str>) -> Vec<(String, HostedSnapshot)> {
        let state = self.state.lock().expect("backup host lock poisoned");
        state
            .hosted
            .iter()
            .filter(|(tag, _)| owner_tag.is_none_or(|t| t == tag.as_str()))
            .map(|(tag, snap)| (tag.clone(), snap.clone()))
            .collect()
    }

    /// Read one hosted fragment back.
    pub fn fetch_fragment(
        &self,
        owner_tag: &str,
        snapshot_id: &str,
        index: u8,
    ) -> P2pResult<Option<Vec<u8>>> {
        let known = {
            let state = self.state.lock().expect("backup host lock poisoned");
            state
                .hosted
                .get(owner_tag)
                .filter(|s| s.snapshot_id == snapshot_id)
                .map(|s| s.fragments.iter().any(|f| f.index == index))
                .unwrap_or(false)
        };
        if !known {
            return Ok(None);
        }
        match std::fs::read(self.fragment_path(owner_tag, snapshot_id, index)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) => Err(P2pError::SyncError(format!("fragment read: {e}"))),
        }
    }

    /// Per-owner accounting for the reciprocity display.
    pub fn accounting(&self) -> Vec<HostedAccount> {
        let state = self.state.lock().expect("backup host lock poisoned");
        state
            .hosted
            .iter()
            .map(|(tag, s)| HostedAccount {
                owner_tag: tag.clone(),
                snapshot_id: s.snapshot_id.clone(),
                epoch: s.epoch,
                fragment_count: s.fragments.len(),
                total_bytes: s.total_bytes,
            })
            .collect()
    }

    // ----- Guardian key shards (the protected half) -----

    /// Store a delivered guardian shard. Newest epoch per (for_user)
    /// replaces older ones.
    pub fn store_guardian_shard(
        &self,
        shard_id: &str,
        for_user: &str,
        epoch: u32,
        shard_data: &str,
    ) -> P2pResult<bool> {
        let mut state = self.state.lock().expect("backup host lock poisoned");
        if state
            .shards
            .iter()
            .any(|s| s.for_user == for_user && s.epoch > epoch)
        {
            return Ok(false);
        }
        state
            .shards
            .retain(|s| !(s.for_user == for_user && s.epoch <= epoch));
        state.shards.push(HeldGuardianShard {
            shard_id: shard_id.to_string(),
            for_user: for_user.to_string(),
            epoch,
            shard_data: shard_data.to_string(),
            received_at: chrono::Utc::now().to_rfc3339(),
            release_requested_at: None,
            release_request_id: None,
            release_approved: false,
        });
        self.save_state(&state)?;
        Ok(true)
    }

    /// Handle an incoming recovery request for a held shard. Returns the
    /// shard data IF (already approved by this device's user) AND (the
    /// release delay has elapsed since the FIRST request); otherwise
    /// records/keeps the pending request and returns None. The caller
    /// surfaces a `ShardRequested`-style event so the user can approve.
    pub fn request_shard_release(
        &self,
        request_id: &str,
        for_user: &str,
        epoch: u32,
    ) -> P2pResult<Option<String>> {
        let mut state = self.state.lock().expect("backup host lock poisoned");
        let delay = chrono::Duration::hours(self.release_delay_hours);
        let now = chrono::Utc::now();

        let Some(shard) = state
            .shards
            .iter_mut()
            .find(|s| s.for_user == for_user && s.epoch == epoch)
        else {
            return Ok(None);
        };

        if shard.release_requested_at.is_none() {
            shard.release_requested_at = Some(now.to_rfc3339());
            shard.release_request_id = Some(request_id.to_string());
            self.save_state(&state)?;
            return Ok(None);
        }

        let requested_at = shard
            .release_requested_at
            .as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&chrono::Utc));
        let delay_elapsed = requested_at.is_some_and(|t| now >= t + delay);

        if shard.release_approved && delay_elapsed {
            Ok(Some(shard.shard_data.clone()))
        } else {
            Ok(None)
        }
    }

    /// This device's user approves releasing the shard held for
    /// `for_user` (epoch). The shard still waits out the release delay.
    pub fn approve_shard_release(&self, for_user: &str, epoch: u32) -> P2pResult<bool> {
        let mut state = self.state.lock().expect("backup host lock poisoned");
        let Some(shard) = state
            .shards
            .iter_mut()
            .find(|s| s.for_user == for_user && s.epoch == epoch)
        else {
            return Ok(false);
        };
        shard.release_approved = true;
        self.save_state(&state)?;
        Ok(true)
    }

    /// Deny / revoke a pending release (clears the request + approval).
    pub fn deny_shard_release(&self, for_user: &str, epoch: u32) -> P2pResult<bool> {
        let mut state = self.state.lock().expect("backup host lock poisoned");
        let Some(shard) = state
            .shards
            .iter_mut()
            .find(|s| s.for_user == for_user && s.epoch == epoch)
        else {
            return Ok(false);
        };
        shard.release_requested_at = None;
        shard.release_request_id = None;
        shard.release_approved = false;
        self.save_state(&state)?;
        Ok(true)
    }

    /// Shards with a pending, not-yet-approved release request (for UI).
    pub fn pending_release_requests(&self) -> Vec<HeldGuardianShard> {
        let state = self.state.lock().expect("backup host lock poisoned");
        state
            .shards
            .iter()
            .filter(|s| s.release_requested_at.is_some() && !s.release_approved)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_host(quota: u64, delay_hours: i64) -> (BackupHost, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "sovereign-backup-host-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        (BackupHost::open_with_delay(dir.clone(), quota, delay_hours), dir)
    }

    fn frag(bytes: &[u8]) -> String {
        sha256_hex(bytes)
    }

    #[test]
    fn store_list_fetch_roundtrip_and_persistence() {
        let (host, dir) = temp_host(1024, 72);
        let data = b"fragment-zero".to_vec();
        assert!(host
            .store_fragment("tagA", "snap1", 1, "{m}", "c2FsdA", 0, &data, &frag(&data))
            .unwrap());

        let listed = host.list_hosted(Some("tagA"));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0, "tagA");
        assert_eq!(listed[0].1.epoch, 1);
        assert_eq!(listed[0].1.manifest_json, "{m}");
        assert_eq!(listed[0].1.salt_b64, "c2FsdA");

        let got = host.fetch_fragment("tagA", "snap1", 0).unwrap().unwrap();
        assert_eq!(got, data);
        assert!(host.fetch_fragment("tagA", "snap1", 9).unwrap().is_none());
        assert!(host.fetch_fragment("tagB", "snap1", 0).unwrap().is_none());

        // Reopen from disk — state survives.
        let host2 = BackupHost::open_with_delay(dir.clone(), 1024, 72);
        assert_eq!(host2.list_hosted(None).len(), 1);
        assert_eq!(host2.fetch_fragment("tagA", "snap1", 0).unwrap().unwrap(), data);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn digest_mismatch_quota_and_epoch_policies() {
        let (host, dir) = temp_host(20, 72);

        // Wrong digest → rejected.
        assert!(!host
            .store_fragment("tag", "s1", 1, "{}", "", 0, b"abc", "deadbeef")
            .unwrap());

        // Quota: 20 bytes total. First 12-byte fragment fits...
        let a = b"twelve-bytes".to_vec();
        assert!(host
            .store_fragment("tag", "s1", 1, "{}", "", 0, &a, &frag(&a))
            .unwrap());
        // ...second would exceed → rejected.
        let b = b"twelve-bytes".to_vec();
        assert!(!host
            .store_fragment("tag", "s1", 1, "{}", "", 1, &b, &frag(&b))
            .unwrap());

        // Older epoch → rejected; newer epoch replaces.
        let c = b"x".to_vec();
        assert!(!host
            .store_fragment("tag", "s0", 0, "{}", "", 0, &c, &frag(&c))
            .unwrap());
        assert!(host
            .store_fragment("tag", "s2", 2, "{}", "", 0, &c, &frag(&c))
            .unwrap());
        let listed = host.list_hosted(Some("tag"));
        assert_eq!(listed[0].1.epoch, 2);
        assert_eq!(listed[0].1.fragments.len(), 1);
        // The replaced snapshot's fragment is gone.
        assert!(host.fetch_fragment("tag", "s1", 0).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn guardian_shard_release_requires_approval_and_delay() {
        // Delay 0 hours → the delay gate passes immediately once a
        // request has been recorded, isolating the approval logic.
        let (host, dir) = temp_host(1024, 0);
        assert!(host.store_guardian_shard("sh1", "tagA", 1, "SHARD").unwrap());

        // First request: records the pending request, returns nothing.
        assert!(host.request_shard_release("req1", "tagA", 1).unwrap().is_none());
        assert_eq!(host.pending_release_requests().len(), 1);

        // Still nothing without user approval (even with delay elapsed).
        assert!(host.request_shard_release("req1", "tagA", 1).unwrap().is_none());

        // Approve → released on the next poll.
        assert!(host.approve_shard_release("tagA", 1).unwrap());
        assert_eq!(
            host.request_shard_release("req1", "tagA", 1).unwrap().as_deref(),
            Some("SHARD")
        );

        // Deny revokes everything.
        assert!(host.deny_shard_release("tagA", 1).unwrap());
        assert!(host.request_shard_release("req2", "tagA", 1).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn guardian_shard_delay_gate_holds_before_deadline() {
        // 72h delay: approval alone must NOT release.
        let (host, dir) = temp_host(1024, 72);
        host.store_guardian_shard("sh1", "tagA", 1, "SHARD").unwrap();
        assert!(host.request_shard_release("req1", "tagA", 1).unwrap().is_none());
        host.approve_shard_release("tagA", 1).unwrap();
        assert!(
            host.request_shard_release("req1", "tagA", 1).unwrap().is_none(),
            "approved but inside the 72h window must stay locked"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_user_shard_requests_return_nothing() {
        let (host, dir) = temp_host(1024, 0);
        assert!(host.request_shard_release("req", "nobody", 1).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
