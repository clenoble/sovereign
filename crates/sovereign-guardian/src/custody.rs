//! Shard custody — the guardian's whole reason to exist.
//!
//! One install can guard for several people. Metadata (who, since when,
//! which epoch) lives in `duties.json`; the shard bytes themselves are
//! sealed per-duty under a per-install random custody key and stored as
//! separate files, so listing duties never touches key material. See the
//! crate docs for the honest scope of this at-rest model.
//!
//! Rotation rule: enrolling again for the same `owner_tag` replaces the
//! existing duty — after a re-split (lost guardian, epoch bump) the old
//! shard is dead weight and keeping it would only confuse heartbeats.

use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as B64;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sovereign_p2p::guardian_enroll::GuardianGrant;
use zeroize::Zeroize;

use crate::error::{GuardianError, GuardianResult};

const CUSTODY_KEY_FILE: &str = "custody.key";
const DUTIES_FILE: &str = "duties.json";
const SHARDS_DIR: &str = "shards";

/// One guarding relationship: "I hold a shard for this person."
/// Metadata only — the shard itself is sealed on disk, never in here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianDuty {
    /// Random stable id; names the sealed shard file.
    pub duty_id: String,
    pub owner_tag: String,
    pub owner_label: String,
    /// The owner device's PeerId recorded at enrollment (heartbeats and
    /// recovery requests are expected from/for this owner).
    pub owner_peer_id: String,
    pub shard_id: String,
    pub epoch: u32,
    pub threshold: u8,
    pub total: u8,
    /// ISO-8601.
    pub enrolled_at: String,
    /// Last successful heartbeat answered (G3 fills this in).
    pub last_heartbeat_at: Option<String>,
}

pub struct CustodyStore {
    dir: PathBuf,
    custody_key: [u8; 32],
    duties: Vec<GuardianDuty>,
}

impl Drop for CustodyStore {
    fn drop(&mut self) {
        self.custody_key.zeroize();
    }
}

impl CustodyStore {
    /// Open (or initialize) the custody store in `dir`.
    pub fn open(dir: &Path) -> GuardianResult<Self> {
        std::fs::create_dir_all(dir.join(SHARDS_DIR))?;
        let key_path = dir.join(CUSTODY_KEY_FILE);
        let custody_key: [u8; 32] = if key_path.exists() {
            std::fs::read(&key_path)?
                .as_slice()
                .try_into()
                .map_err(|_| GuardianError::Custody("custody key corrupt".into()))?
        } else {
            let mut key = [0u8; 32];
            rand::rng().fill_bytes(&mut key);
            sovereign_crypto::fs_private::write_private(&key_path, key)?;
            key
        };
        let duties_path = dir.join(DUTIES_FILE);
        let duties = if duties_path.exists() {
            serde_json::from_slice(&std::fs::read(&duties_path)?)
                .map_err(|e| GuardianError::Custody(format!("duties.json corrupt: {e}")))?
        } else {
            Vec::new()
        };
        Ok(Self {
            dir: dir.to_path_buf(),
            custody_key,
            duties,
        })
    }

    /// Record a freshly granted duty, sealing the shard to disk. Called
    /// from the enrollment `persist_grant` callback — the custody receipt
    /// only goes out after this returns Ok. Replaces any existing duty
    /// for the same `owner_tag`.
    pub fn add_duty(&mut self, grant: &GuardianGrant, owner_peer_id: &str) -> GuardianResult<GuardianDuty> {
        let mut shard_bytes = B64
            .decode(&grant.shard_b64)
            .map_err(|e| GuardianError::Custody(format!("shard base64: {e}")))?;
        if shard_bytes.is_empty() {
            return Err(GuardianError::Custody("empty shard in grant".into()));
        }

        let mut id_bytes = [0u8; 16];
        rand::rng().fill_bytes(&mut id_bytes);
        let duty = GuardianDuty {
            duty_id: B64URL.encode(id_bytes),
            owner_tag: grant.owner_tag.clone(),
            owner_label: grant.owner_label.clone(),
            owner_peer_id: owner_peer_id.to_string(),
            shard_id: grant.shard_id.clone(),
            epoch: grant.epoch,
            threshold: grant.threshold,
            total: grant.total,
            enrolled_at: chrono::Utc::now().to_rfc3339(),
            last_heartbeat_at: None,
        };

        // Seal shard: file = nonce(24) || ciphertext.
        let (ct, nonce) = sovereign_crypto::aead::encrypt(&shard_bytes, &self.custody_key)?;
        shard_bytes.zeroize();
        let mut file_bytes = Vec::with_capacity(24 + ct.len());
        file_bytes.extend_from_slice(&nonce);
        file_bytes.extend_from_slice(&ct);
        sovereign_crypto::fs_private::write_private(&self.shard_path(&duty.duty_id), &file_bytes)?;

        // Replace any prior duty for the same owner (rotation / retry).
        if let Some(old) = self
            .duties
            .iter()
            .position(|d| d.owner_tag == duty.owner_tag)
        {
            let old = self.duties.remove(old);
            let _ = std::fs::remove_file(self.shard_path(&old.duty_id));
        }
        self.duties.push(duty.clone());
        self.persist_duties()?;
        Ok(duty)
    }

    pub fn duties(&self) -> &[GuardianDuty] {
        &self.duties
    }

    pub fn duty_for_owner(&self, owner_tag: &str) -> Option<&GuardianDuty> {
        self.duties.iter().find(|d| d.owner_tag == owner_tag)
    }

    /// Unseal the shard for one duty. Caller must zeroize.
    pub fn shard_bytes(&self, duty_id: &str) -> GuardianResult<Vec<u8>> {
        let bytes = std::fs::read(self.shard_path(duty_id))?;
        if bytes.len() < 25 {
            return Err(GuardianError::Custody("sealed shard truncated".into()));
        }
        let mut nonce = [0u8; 24];
        nonce.copy_from_slice(&bytes[..24]);
        Ok(sovereign_crypto::aead::decrypt(
            &bytes[24..],
            &nonce,
            &self.custody_key,
        )?)
    }

    /// Answer a heartbeat challenge for one duty (G3, and the enrollment
    /// e2e test): MAC keyed by the held shard, epoch-bound.
    pub fn prove_possession(&self, duty_id: &str, nonce: &[u8]) -> GuardianResult<String> {
        let duty = self
            .duties
            .iter()
            .find(|d| d.duty_id == duty_id)
            .ok_or_else(|| GuardianError::Custody(format!("unknown duty {duty_id}")))?;
        let mut shard = self.shard_bytes(duty_id)?;
        let proof = sovereign_crypto::guardian::pop::prove_possession(&shard, duty.epoch, nonce)?;
        shard.zeroize();
        Ok(proof)
    }

    /// Drop a duty and its sealed shard (owner rotated us out, or the
    /// friendship ended — it happens).
    pub fn remove_duty(&mut self, duty_id: &str) -> GuardianResult<bool> {
        let Some(pos) = self.duties.iter().position(|d| d.duty_id == duty_id) else {
            return Ok(false);
        };
        let old = self.duties.remove(pos);
        let _ = std::fs::remove_file(self.shard_path(&old.duty_id));
        self.persist_duties()?;
        Ok(true)
    }

    fn shard_path(&self, duty_id: &str) -> PathBuf {
        self.dir.join(SHARDS_DIR).join(format!("{duty_id}.seal"))
    }

    fn persist_duties(&self) -> GuardianResult<()> {
        let json = serde_json::to_vec_pretty(&self.duties)
            .map_err(|e| GuardianError::Custody(format!("duties encode: {e}")))?;
        sovereign_crypto::fs_private::write_private(&self.dir.join(DUTIES_FILE), &json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(owner_tag: &str, epoch: u32) -> GuardianGrant {
        GuardianGrant {
            shard_b64: B64.encode([0xAB; 33]),
            shard_id: format!("shard-{owner_tag}-{epoch}"),
            owner_tag: owner_tag.into(),
            owner_label: "Céline".into(),
            epoch,
            threshold: 3,
            total: 5,
        }
    }

    #[test]
    fn add_persist_reopen_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = CustodyStore::open(dir.path()).unwrap();
        let duty = store.add_duty(&grant("tag-1", 1), "12D3KooWOwner").unwrap();
        assert_eq!(store.duties().len(), 1);

        // Reopen: duty metadata and shard both survive.
        drop(store);
        let store = CustodyStore::open(dir.path()).unwrap();
        assert_eq!(store.duties().len(), 1);
        assert_eq!(store.duties()[0].owner_label, "Céline");
        let shard = store.shard_bytes(&duty.duty_id).unwrap();
        assert_eq!(shard, vec![0xAB; 33]);
    }

    #[test]
    fn same_owner_reenroll_replaces_duty() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = CustodyStore::open(dir.path()).unwrap();
        let old = store.add_duty(&grant("tag-1", 1), "12D3KooWOwner").unwrap();
        let new = store.add_duty(&grant("tag-1", 2), "12D3KooWOwner").unwrap();
        assert_eq!(store.duties().len(), 1, "rotation replaces, never stacks");
        assert_eq!(store.duties()[0].epoch, 2);
        assert!(store.shard_bytes(&old.duty_id).is_err(), "old shard gone");
        assert!(store.shard_bytes(&new.duty_id).is_ok());
    }

    #[test]
    fn distinct_owners_coexist() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = CustodyStore::open(dir.path()).unwrap();
        store.add_duty(&grant("tag-1", 1), "12D3KooWA").unwrap();
        store.add_duty(&grant("tag-2", 1), "12D3KooWB").unwrap();
        assert_eq!(store.duties().len(), 2, "one install guards for several people");
    }

    #[test]
    fn possession_proof_verifies_against_dealt_shard() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = CustodyStore::open(dir.path()).unwrap();
        let duty = store.add_duty(&grant("tag-1", 3), "12D3KooWOwner").unwrap();
        let nonce = [7u8; 32];
        let proof = store.prove_possession(&duty.duty_id, &nonce).unwrap();
        // Owner side: verifies against its own copy of the dealt shard.
        assert!(sovereign_crypto::guardian::pop::verify_possession(
            &[0xAB; 33],
            3,
            &nonce,
            &proof
        )
        .unwrap());
        // Wrong epoch (pre-rotation replay) fails.
        assert!(!sovereign_crypto::guardian::pop::verify_possession(
            &[0xAB; 33],
            4,
            &nonce,
            &proof
        )
        .unwrap());
    }

    #[test]
    fn remove_duty_deletes_shard() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = CustodyStore::open(dir.path()).unwrap();
        let duty = store.add_duty(&grant("tag-1", 1), "12D3KooWOwner").unwrap();
        assert!(store.remove_duty(&duty.duty_id).unwrap());
        assert!(store.duties().is_empty());
        assert!(store.shard_bytes(&duty.duty_id).is_err());
        // Reopen confirms persistence of the removal.
        drop(store);
        let store = CustodyStore::open(dir.path()).unwrap();
        assert!(store.duties().is_empty());
    }

    #[test]
    fn sealed_shard_is_not_plaintext_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = CustodyStore::open(dir.path()).unwrap();
        let duty = store.add_duty(&grant("tag-1", 1), "12D3KooWOwner").unwrap();
        let on_disk = std::fs::read(store.shard_path(&duty.duty_id)).unwrap();
        assert!(
            !on_disk.windows(33).any(|w| w == [0xAB; 33]),
            "shard bytes must not appear in the sealed file"
        );
    }
}
