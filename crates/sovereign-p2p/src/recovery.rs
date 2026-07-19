//! Workstream B — the restore engine: a **resumable** account-recovery
//! driver over the P4.3 primitives in [`crate::backup_client`].
//!
//! Recovery spans **days** (the guardian 72h anti-coercion delay), so it
//! cannot be one blocking call. It is a persisted state machine: the app
//! calls [`RecoveryState::poll_round`] on a timer (and persists the state
//! to `recovery_state.json` between calls, surviving app restarts), until
//! [`RecoveryState::ready_to_assemble`], then [`RecoveryState::finalize`].
//!
//! Flow (mirrors the module docs of `backup_client`):
//!   1. poll guardians (`RequestShard`) — each poll may return a
//!      [`BackupGuardianPayload`] once that guardian approved + 72h elapsed.
//!      The **first** payload bootstraps `owner_tag` + `salt` + `manifest`.
//!   2. once salt is known: passphrase + salt → MasterKey → AccountKey →
//!      re-derive the backup signing pubkey (the A1 origin check) and the
//!      owner tag (must match the manifest).
//!   3. fetch ciphertext fragments from hosts until `data_fragments` valid.
//!   4. finalize: `assemble_snapshot` (A1 gauntlet) → `restore_snapshot`.
//!
//! The pure state transitions (what to request next, when enough is
//! collected, phase progression) are unit-tested without a network.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sovereign_db::GraphDB;

use crate::backup::{restore_snapshot, BackupGuardianPayload, BackupManifest};
use crate::backup_client::{assemble_snapshot, fetch_fragments, list_backups, request_guardian_shard};
use crate::error::{P2pError, P2pResult};

/// Coarse phase for the recovery UI (matches the command contract §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryPhase {
    /// No guardian has released a shard yet — no manifest, nothing to show.
    Locating,
    /// Manifest known; still gathering the guardian key-share threshold.
    AwaitingShards,
    /// Threshold shards held; gathering ciphertext fragments from hosts.
    AwaitingFragments,
    /// Enough of both — ready for `finalize`.
    Assembling,
    /// `assemble_snapshot` verified + unsealed (A1 passed).
    Verified,
    /// Snapshot written into the fresh DB.
    Installed,
    /// Hard failure (bad passphrase, verification, etc.); see `error`.
    Failed,
}

/// One guardian's recovery-request bookkeeping (drives the 72h UI line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianProgress {
    /// Guardian id (peer id) from the manifest's `guardian_shards`.
    pub guardian_id: String,
    /// Dialable address the app resolved for this guardian (via relay/seed).
    pub addr: String,
    /// True once this guardian released its shard.
    pub released: bool,
}

/// Persisted recovery progress — serialized to `recovery_state.json` so a
/// multi-day recovery survives app restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryState {
    pub recovery_id: String,
    pub phase: RecoveryPhase,
    /// Learned from the first guardian payload; also re-derived from
    /// passphrase+salt at finalize and checked against this.
    pub owner_tag: Option<String>,
    pub salt_b64: Option<String>,
    pub manifest: Option<BackupManifest>,
    /// Collected guardian payloads, keyed by guardian_id (dedup).
    pub shards: BTreeMap<String, BackupGuardianPayload>,
    /// Collected, digest-verified fragments, keyed by index (dedup).
    pub fragments: BTreeMap<u8, crate::backup::BackupFragment>,
    pub guardians: Vec<GuardianProgress>,
    /// Dialable fragment-host addresses (own devices; Phase 2 adds mesh).
    pub host_addrs: Vec<String>,
    pub manifest_verified: Option<bool>,
    pub error: Option<String>,
}

impl RecoveryState {
    /// Begin a recovery. `owner_tag` comes from the recovery card (it is
    /// the non-secret identifier hosts index backups by — without it the
    /// first guardian request can never register the 72h window, because
    /// hosts match shards by exact tag). `guardians` and `host_addrs` are
    /// dialable `/p2p/<peer>` addresses the app resolved (via the seed
    /// relay / recorded enrollment addresses). Fixed request-id per
    /// guardian so repeated polls map to the same 72h window.
    pub fn new(
        recovery_id: String,
        owner_tag: String,
        guardians: Vec<(String, String)>, // (guardian_id, addr)
        host_addrs: Vec<String>,
    ) -> Self {
        Self {
            recovery_id,
            phase: RecoveryPhase::Locating,
            owner_tag: Some(owner_tag),
            salt_b64: None,
            manifest: None,
            shards: BTreeMap::new(),
            fragments: BTreeMap::new(),
            guardians: guardians
                .into_iter()
                .map(|(guardian_id, addr)| GuardianProgress { guardian_id, addr, released: false })
                .collect(),
            host_addrs,
            manifest_verified: None,
            error: None,
        }
    }

    /// Shamir threshold (from the manifest once known).
    pub fn shards_needed(&self) -> u8 {
        self.manifest.as_ref().map(|m| m.key_threshold).unwrap_or(0)
    }
    pub fn shards_collected(&self) -> usize {
        self.shards.len()
    }
    pub fn fragments_needed(&self) -> u8 {
        self.manifest.as_ref().map(|m| m.data_fragments).unwrap_or(0)
    }
    pub fn fragments_collected(&self) -> usize {
        self.fragments.len()
    }
    pub fn have_enough_shards(&self) -> bool {
        self.shards_needed() > 0 && self.shards_collected() >= self.shards_needed() as usize
    }
    pub fn have_enough_fragments(&self) -> bool {
        self.fragments_needed() > 0 && self.fragments_collected() >= self.fragments_needed() as usize
    }
    pub fn ready_to_assemble(&self) -> bool {
        self.have_enough_shards() && self.have_enough_fragments()
    }

    /// Fold in a released guardian payload: dedup, bootstrap
    /// owner_tag/salt/manifest from the first one, mark the guardian.
    pub fn ingest_shard(&mut self, guardian_id: &str, payload: BackupGuardianPayload) -> P2pResult<()> {
        match &self.owner_tag {
            // A payload for someone else's tag must not poison this
            // recovery's salt/manifest (a compromised guardian could
            // otherwise redirect the restore).
            Some(tag) if payload.owner_tag != *tag => {
                return Err(P2pError::SyncError(format!(
                    "guardian {guardian_id} returned a payload for a different owner tag"
                )));
            }
            Some(_) => {}
            None => self.owner_tag = Some(payload.owner_tag.clone()),
        }
        if self.salt_b64.is_none() {
            self.salt_b64 = Some(payload.salt_b64.clone());
        }
        if self.manifest.is_none() {
            self.manifest = Some(BackupManifest::from_json(&payload.manifest_json)?);
        }
        self.shards.insert(guardian_id.to_string(), payload);
        if let Some(g) = self.guardians.iter_mut().find(|g| g.guardian_id == guardian_id) {
            g.released = true;
        }
        Ok(())
    }

    /// Recompute the coarse phase from what's collected (idempotent).
    pub fn recompute_phase(&mut self) {
        if self.phase == RecoveryPhase::Failed
            || self.phase == RecoveryPhase::Installed
            || self.phase == RecoveryPhase::Verified
        {
            return;
        }
        self.phase = if self.ready_to_assemble() {
            RecoveryPhase::Assembling
        } else if self.manifest.is_some() && self.have_enough_shards() {
            RecoveryPhase::AwaitingFragments
        } else if self.manifest.is_some() {
            RecoveryPhase::AwaitingShards
        } else {
            RecoveryPhase::Locating
        };
    }

    /// One network round (call on a timer, persist state after):
    /// poll every not-yet-released guardian, then — once the manifest is
    /// known and shards suffice — fetch any missing fragments from hosts.
    /// Errors on individual peers are logged and skipped, never fatal.
    pub async fn poll_round(&mut self, timeout: Duration) {
        // Locate first: the manifest + salt travel with the fragments on
        // every host (`ListBackups` by exact owner tag — the tag comes
        // from the recovery card). Guardian requests are deferred until
        // the manifest is known: the host registers the 72h window
        // against an exact (tag, epoch), so a request sent before we
        // know the epoch could never match and would only produce a
        // confusing approval prompt on the guardian's device.
        if self.manifest.is_none() {
            let tag = self.owner_tag.clone().unwrap_or_default();
            if !tag.is_empty() {
                for addr in self.host_addrs.clone() {
                    match list_backups(&addr, Some(tag.clone()), timeout).await {
                        Ok(list) => {
                            let Some(info) = list.first() else { continue };
                            match BackupManifest::from_json(&info.manifest_json) {
                                Ok(m) if m.owner_tag == tag => {
                                    self.salt_b64 = Some(info.salt_b64.clone());
                                    self.manifest = Some(m);
                                    break;
                                }
                                Ok(_) => tracing::warn!(
                                    target: "recovery",
                                    %addr,
                                    "host listed a manifest for a different owner tag — skipped"
                                ),
                                Err(e) => warn_skip("parse listed manifest", &addr, e),
                            }
                        }
                        Err(e) => warn_skip("list backups", &addr, e),
                    }
                }
            }
        }
        let Some(manifest_epoch) = self.manifest.as_ref().map(|m| m.epoch) else {
            self.recompute_phase();
            return;
        };
        let owner_tag = self.owner_tag.clone();

        // Guardians: fixed request-id per guardian → same 72h window.
        let pending: Vec<(String, String)> = self
            .guardians
            .iter()
            .filter(|g| !g.released)
            .map(|g| (g.guardian_id.clone(), g.addr.clone()))
            .collect();
        for (gid, addr) in pending {
            let req_id = format!("{}-{}", self.recovery_id, gid);
            let (tag, epoch) = (owner_tag.clone().unwrap_or_default(), manifest_epoch);
            match request_guardian_shard(&addr, &req_id, &tag, epoch, timeout).await {
                Ok(Some(payload)) => {
                    if let Err(e) = self.ingest_shard(&gid, payload) {
                        warn_skip("ingest guardian payload", &gid, e);
                    }
                }
                Ok(None) => {} // still pending approval / 72h
                Err(e) => warn_skip("request guardian shard", &addr, e),
            }
        }

        // Fragments: only once we have the manifest + threshold shards.
        if self.manifest.is_some() && self.have_enough_shards() && !self.have_enough_fragments() {
            let manifest = self.manifest.clone().expect("checked");
            let owner_tag = self.owner_tag.clone().expect("set with manifest");
            let want: Vec<u8> = (0..(manifest.data_fragments + manifest.parity_fragments))
                .filter(|i| !self.fragments.contains_key(i))
                .collect();
            for addr in self.host_addrs.clone() {
                if self.have_enough_fragments() {
                    break;
                }
                match fetch_fragments(&addr, &owner_tag, &manifest.snapshot_id, &want, &manifest, timeout)
                    .await
                {
                    Ok(frags) => {
                        for f in frags {
                            self.fragments.insert(f.index, f);
                        }
                    }
                    Err(e) => warn_skip("fetch fragments", &addr, e),
                }
            }
        }
        self.recompute_phase();
    }

    /// The recovered MasterKey salt, decoded. Available once the first
    /// guardian payload arrived (payloads carry the salt).
    pub fn recovered_salt(&self) -> P2pResult<Vec<u8>> {
        use base64::Engine;
        let salt_b64 = self
            .salt_b64
            .clone()
            .ok_or_else(|| P2pError::SyncError("no salt yet".into()))?;
        base64::engine::general_purpose::STANDARD
            .decode(&salt_b64)
            .map_err(|e| P2pError::SyncError(format!("salt base64: {e}")))
    }

    /// Non-destructive passphrase check: re-derive passphrase+recovered-salt
    /// → AccountKey and compare its backup tag to the manifest's owner tag.
    /// `Ok(false)` = wrong passphrase — the recovery itself stays valid
    /// (shards/fragments keep), the user just retypes. Both frontends call
    /// this before creating any account state, so a typo never touches disk.
    pub fn passphrase_matches(&self, passphrase: &[u8]) -> P2pResult<bool> {
        let manifest = self
            .manifest
            .as_ref()
            .ok_or_else(|| P2pError::SyncError("no manifest yet".into()))?;
        let salt = self.recovered_salt()?;
        let mk = sovereign_crypto::master_key::MasterKey::from_passphrase(passphrase, &salt)
            .map_err(|e| P2pError::SyncError(format!("master key: {e}")))?;
        let ak = sovereign_crypto::account_key::AccountKey::derive(&mk)
            .map_err(|e| P2pError::SyncError(format!("account key: {e}")))?;
        Ok(ak.derive_backup_tag() == manifest.owner_tag)
    }

    /// Final offline step: re-derive the AccountKey from the passphrase +
    /// recovered salt, verify it reproduces the manifest's owner tag AND
    /// backup signer (A1 origin), assemble+verify+unseal, restore into
    /// `db`. Sets phase to Installed on success and Failed on assembly
    /// failure. A wrong passphrase does NOT fail the recovery — the shards
    /// stay valid and the phase is left untouched so the user can retype.
    pub async fn finalize(&mut self, db: &dyn GraphDB, passphrase: &[u8]) -> P2pResult<u64> {
        let manifest = self
            .manifest
            .clone()
            .ok_or_else(|| P2pError::SyncError("no manifest yet".into()))?;
        let salt = self.recovered_salt()?;

        // Owner-tag check: wrong passphrase → wrong tag → abort early,
        // WITHOUT failing the recovery (retryable — see doc above).
        if !self.passphrase_matches(passphrase)? {
            self.error = Some("passphrase does not match this backup (owner tag mismatch)".into());
            return Err(P2pError::SyncError("owner tag mismatch — wrong passphrase".into()));
        }
        self.error = None;

        // passphrase + salt → MasterKey → AccountKey.
        let mk = sovereign_crypto::master_key::MasterKey::from_passphrase(passphrase, &salt)
            .map_err(|e| P2pError::SyncError(format!("master key: {e}")))?;
        let ak = sovereign_crypto::account_key::AccountKey::derive(&mk)
            .map_err(|e| P2pError::SyncError(format!("account key: {e}")))?;

        // A1 origin: the signer we expect is the one this account derives.
        let expected_signer =
            sovereign_crypto::backup_signing::verifying_key_b64(&ak.derive_backup_signing_key());

        let fragments: Vec<_> = self.fragments.values().cloned().collect();
        let payloads: Vec<_> = self.shards.values().cloned().collect();
        let snapshot = match assemble_snapshot(&manifest, &fragments, &payloads, Some(&expected_signer))
        {
            Ok(s) => s,
            Err(e) => {
                self.phase = RecoveryPhase::Failed;
                self.manifest_verified = Some(false);
                self.error = Some(format!("assembly/verify failed: {e}"));
                return Err(e);
            }
        };
        self.manifest_verified = Some(true);
        self.phase = RecoveryPhase::Verified;

        let written = restore_snapshot(db, &snapshot).await?;
        self.phase = RecoveryPhase::Installed;
        Ok(written)
    }
}

fn warn_skip(what: &str, who: &str, e: P2pError) {
    tracing::warn!(target: "recovery", "{what} for {who} failed (skipped): {e}");
}

// ---------------------------------------------------------------------------
// Shared frontend seam (the shell calls these in-process; the Tauri commands
// in sovereign-app are thin wrappers over the same functions — one engine,
// two faces). Persistence lives here so both frontends resume identically.
// ---------------------------------------------------------------------------

/// UI-facing status snapshot — field-for-field the command contract §2
/// `RecoveryStatusDto`. Serialize directly to the Tauri frontend; the shell
/// reads the struct in-process.
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryStatusDto {
    pub recovery_id: String,
    pub phase: RecoveryPhase,
    pub shards_collected: u8,
    pub shards_needed: u8,
    pub fragments_collected: u8,
    pub fragments_needed: u8,
    pub guardians: Vec<GuardianStatusDto>,
    pub manifest_verified: Option<bool>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuardianStatusDto {
    pub guardian_label: String,
    /// "pending" | "released" (the guardian-side approve state isn't
    /// visible to the owner until release; "approved" arrives with G3).
    pub state: &'static str,
    /// Filled by G3 heartbeat data later; None until then.
    pub hours_remaining: Option<u32>,
}

impl RecoveryState {
    /// The UI status snapshot (contract §2).
    pub fn status(&self) -> RecoveryStatusDto {
        RecoveryStatusDto {
            recovery_id: self.recovery_id.clone(),
            phase: self.phase,
            shards_collected: self.shards_collected() as u8,
            shards_needed: self.shards_needed(),
            fragments_collected: self.fragments_collected() as u8,
            fragments_needed: self.fragments_needed(),
            guardians: self
                .guardians
                .iter()
                .map(|g| GuardianStatusDto {
                    guardian_label: g.guardian_id.clone(),
                    state: if g.released { "released" } else { "pending" },
                    hours_remaining: None,
                })
                .collect(),
            manifest_verified: self.manifest_verified,
            error: self.error.clone(),
        }
    }

    /// Persist to `<dir>/recovery_state.json` (atomic-ish: tmp + rename).
    pub fn save(&self, dir: &std::path::Path) -> P2pResult<()> {
        std::fs::create_dir_all(dir)
            .map_err(|e| P2pError::SyncError(format!("recovery dir: {e}")))?;
        let json = serde_json::to_string(self)
            .map_err(|e| P2pError::SyncError(format!("recovery encode: {e}")))?;
        let tmp = dir.join("recovery_state.json.tmp");
        std::fs::write(&tmp, json)
            .map_err(|e| P2pError::SyncError(format!("recovery write: {e}")))?;
        std::fs::rename(&tmp, dir.join("recovery_state.json"))
            .map_err(|e| P2pError::SyncError(format!("recovery rename: {e}")))?;
        Ok(())
    }

    /// Resume a persisted recovery, if one exists.
    pub fn load(dir: &std::path::Path) -> Option<Self> {
        let json = std::fs::read_to_string(dir.join("recovery_state.json")).ok()?;
        serde_json::from_str(&json).ok()
    }

    /// Cancel: remove the persisted state. (Shards a guardian already
    /// released stay released on their side — cancel is local.)
    pub fn cancel(dir: &std::path::Path) {
        let _ = std::fs::remove_file(dir.join("recovery_state.json"));
    }
}

/// Convenience: resolve the guardian set + host addresses a recovery needs.
/// Phase 1 hosts are the user's own surviving devices; the app supplies
/// their addresses (resolved via the seed relay). Kept here so the app
/// has one place to call. `owner_tag` lets a host be queried before any
/// guardian has released the manifest.
pub async fn discover_hosted(
    host_addrs: &[String],
    owner_tag: &str,
    timeout: Duration,
) -> Vec<(String, crate::protocol::HostedBackupInfo)> {
    let mut found = Vec::new();
    for addr in host_addrs {
        if let Ok(list) = list_backups(addr, Some(owner_tag.to_string()), timeout).await {
            for info in list {
                found.push((addr.clone(), info));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(tag: &str, epoch: u32, manifest_json: &str) -> BackupGuardianPayload {
        BackupGuardianPayload {
            schema_version: 1,
            owner_tag: tag.into(),
            epoch,
            key_share_b64: "c2hhcmU=".into(),
            salt_b64: "c2FsdA==".into(),
            manifest_json: manifest_json.into(),
        }
    }

    fn manifest_json(tag: &str, threshold: u8, data: u8) -> String {
        let mut m = BackupManifest {
            snapshot_id: "snap".into(),
            epoch: 1,
            created_at: "t".into(),
            owner_tag: tag.into(),
            ciphertext_digest: "d".into(),
            ciphertext_len: 1,
            nonce_b64: "bg==".into(),
            data_fragments: data,
            parity_fragments: 2,
            fragment_digests: vec![],
            key_threshold: threshold,
            guardian_shards: vec![],
            signer_pubkey_b64: String::new(),
            signature_b64: String::new(),
        };
        m.signature_b64 = "x".into();
        m.to_json().unwrap()
    }

    fn state() -> RecoveryState {
        RecoveryState::new(
            "rec1".into(),
            "tagA".into(),
            vec![
                ("g1".into(), "/ip4/1.1.1.1/udp/1/quic-v1/p2p/x".into()),
                ("g2".into(), "/ip4/1.1.1.2/udp/1/quic-v1/p2p/y".into()),
                ("g3".into(), "/ip4/1.1.1.3/udp/1/quic-v1/p2p/z".into()),
            ],
            vec!["/ip4/2.2.2.2/udp/1/quic-v1/p2p/h".into()],
        )
    }

    #[test]
    fn starts_locating_with_nothing() {
        let mut s = state();
        s.recompute_phase();
        assert_eq!(s.phase, RecoveryPhase::Locating);
        assert_eq!(s.shards_needed(), 0);
        assert!(!s.ready_to_assemble());
    }

    #[test]
    fn first_shard_bootstraps_manifest_and_advances_phase() {
        let mut s = state();
        let mj = manifest_json("tagA", 3, 3);
        s.ingest_shard("g1", payload("tagA", 1, &mj)).unwrap();
        s.recompute_phase();
        assert_eq!(s.owner_tag.as_deref(), Some("tagA"));
        assert_eq!(s.shards_needed(), 3);
        assert_eq!(s.phase, RecoveryPhase::AwaitingShards);
    }

    #[test]
    fn shard_dedup_by_guardian() {
        let mut s = state();
        let mj = manifest_json("tagA", 3, 3);
        s.ingest_shard("g1", payload("tagA", 1, &mj)).unwrap();
        s.ingest_shard("g1", payload("tagA", 1, &mj)).unwrap(); // same guardian
        assert_eq!(s.shards_collected(), 1, "re-poll of one guardian doesn't double-count");
    }

    #[test]
    fn threshold_shards_then_fragments_gate_assembly() {
        let mut s = state();
        let mj = manifest_json("tagA", 3, 3);
        for g in ["g1", "g2", "g3"] {
            s.ingest_shard(g, payload("tagA", 1, &mj)).unwrap();
        }
        s.recompute_phase();
        assert!(s.have_enough_shards());
        assert_eq!(s.phase, RecoveryPhase::AwaitingFragments, "shards done, fragments pending");

        for i in 0..3u8 {
            s.fragments.insert(
                i,
                crate::backup::BackupFragment { index: i, data_b64: "AA==".into(), digest: "d".into() },
            );
        }
        s.recompute_phase();
        assert!(s.ready_to_assemble());
        assert_eq!(s.phase, RecoveryPhase::Assembling);
    }

    #[test]
    fn save_load_roundtrip_and_cancel() {
        let dir = std::env::temp_dir().join(format!(
            "sovereign-recovery-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut s = state();
        let mj = manifest_json("tagA", 3, 3);
        s.ingest_shard("g1", payload("tagA", 1, &mj)).unwrap();
        s.recompute_phase();
        s.save(&dir).unwrap();

        let loaded = RecoveryState::load(&dir).expect("resume");
        assert_eq!(loaded.recovery_id, "rec1");
        assert_eq!(loaded.shards_collected(), 1);
        assert_eq!(loaded.phase, RecoveryPhase::AwaitingShards);

        RecoveryState::cancel(&dir);
        assert!(RecoveryState::load(&dir).is_none(), "cancel removes the state");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_dto_reflects_state() {
        let mut s = state();
        let mj = manifest_json("tagA", 3, 3);
        s.ingest_shard("g2", payload("tagA", 1, &mj)).unwrap();
        s.recompute_phase();
        let dto = s.status();
        assert_eq!(dto.shards_collected, 1);
        assert_eq!(dto.shards_needed, 3);
        assert_eq!(dto.guardians.len(), 3);
        assert_eq!(
            dto.guardians.iter().filter(|g| g.state == "released").count(),
            1
        );
    }

    #[test]
    fn guardian_released_flag_tracks() {
        let mut s = state();
        let mj = manifest_json("tagA", 3, 3);
        s.ingest_shard("g2", payload("tagA", 1, &mj)).unwrap();
        let g2 = s.guardians.iter().find(|g| g.guardian_id == "g2").unwrap();
        assert!(g2.released);
        let g1 = s.guardians.iter().find(|g| g.guardian_id == "g1").unwrap();
        assert!(!g1.released);
    }
}
