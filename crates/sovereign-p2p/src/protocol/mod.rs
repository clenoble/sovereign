pub mod guardian;
pub mod manifest;
pub mod sync;

use serde::{Deserialize, Serialize};

/// Top-level request type for the Sovereign sync protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SovereignRequest {
    /// Request a sync manifest from a peer.
    GetManifest,
    /// Push a sync manifest to a peer.
    PushManifest(manifest::EncryptedManifest),
    /// Request specific commits by ID.
    GetCommits { commit_ids: Vec<String> },
    /// Push encrypted commits to a peer.
    PushCommits { commits: Vec<sync::EncryptedCommit> },
    /// Request specific rows from a non-document table.
    GetRows {
        table: sync::SyncTable,
        ids: Vec<String>,
    },
    /// Push encrypted rows to a peer (LWW resolution server-side).
    PushRows {
        table: sync::SyncTable,
        rows: Vec<sync::EncryptedRow>,
    },
    /// Guardian shard delivery.
    DeliverShard(guardian::ShardDeliveryRequest),
    /// Request a shard for recovery.
    RequestShard(guardian::ShardRecoveryRequest),
    /// Store one backup fragment on this host (P4.2). Paired peers only.
    StoreBackupFragment {
        owner_tag: String,
        snapshot_id: String,
        epoch: u32,
        /// Public manifest + salt travel with every fragment so any
        /// single surviving host can bootstrap a recovery.
        manifest_json: String,
        salt_b64: String,
        index: u8,
        fragment_b64: String,
        digest: String,
    },
    /// List hosted backups (recovery bootstrap; allowed unpaired — the
    /// listing is public-safe by design, see `backup_host` docs).
    ListBackups { owner_tag: Option<String> },
    /// Fetch one hosted fragment back (recovery; allowed unpaired).
    FetchBackupFragment {
        owner_tag: String,
        snapshot_id: String,
        index: u8,
    },
    /// Pairing handshake step 1 (P3.1): a new device opens the
    /// handshake against an active offer.
    PairHello { offer_id: String, device_name: String },
    /// Pairing handshake step 2: prove knowledge of the pairing code
    /// over the issued challenge (see `pairing_offer::proof_mac`).
    PairProof { offer_id: String, proof: Vec<u8> },
    /// Pairing handshake step 3: after deriving its final identity from
    /// the received salt, the new device binds that identity to the
    /// session (see `pairing_offer::confirm_mac`).
    PairComplete {
        offer_id: String,
        final_peer_id: String,
        device_name: String,
        mac: Vec<u8>,
    },
}

/// Top-level response type for the Sovereign sync protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SovereignResponse {
    /// Manifest in response to GetManifest.
    Manifest(manifest::EncryptedManifest),
    /// Acknowledgement.
    Ok,
    /// Commits in response to GetCommits.
    Commits { commits: Vec<sync::EncryptedCommit> },
    /// Rows in response to GetRows.
    Rows {
        table: sync::SyncTable,
        rows: Vec<sync::EncryptedRow>,
    },
    /// Acknowledgement of a PushRows with per-row write/skip counts.
    PushAck { written: u32, skipped: u32 },
    /// Shard delivery acknowledgement.
    ShardAck { accepted: bool },
    /// Shard for recovery.
    ShardData { shard_data: Option<String> },
    /// Ack for `StoreBackupFragment`.
    BackupStored { accepted: bool },
    /// Hosted backups in response to `ListBackups`.
    BackupList { backups: Vec<HostedBackupInfo> },
    /// Fragment bytes in response to `FetchBackupFragment` (None when
    /// this host doesn't hold it).
    BackupFragmentData { fragment_b64: Option<String> },
    /// Error message.
    Error { message: String },
    /// Pairing: challenge nonce in response to a valid `PairHello`.
    PairChallenge { nonce: Vec<u8> },
    /// Pairing: the AccountKey + salt, AEAD-sealed under the handshake
    /// key, released after a valid `PairProof`.
    PairGranted { ciphertext: String, nonce: String },
    /// Pairing: final ack — the existing device has registered the new
    /// device's final peer id as paired.
    PairDone,
    /// Pairing rejected (bad offer/proof/mac, expired, or attempts
    /// exhausted).
    PairRejected { reason: String },
}

/// Wire view of one hosted backup (P4): the public manifest + salt and
/// which fragment indices this host holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostedBackupInfo {
    pub owner_tag: String,
    pub snapshot_id: String,
    pub epoch: u32,
    pub manifest_json: String,
    pub salt_b64: String,
    pub fragment_indices: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serde_roundtrip() {
        let req = SovereignRequest::GetManifest;
        let json = serde_json::to_string(&req).unwrap();
        let back: SovereignRequest = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, SovereignRequest::GetManifest));
    }

    #[test]
    fn response_serde_roundtrip() {
        let resp = SovereignResponse::Ok;
        let json = serde_json::to_string(&resp).unwrap();
        let back: SovereignResponse = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, SovereignResponse::Ok));
    }

    #[test]
    fn error_response() {
        let resp = SovereignResponse::Error {
            message: "not found".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("not found"));
    }
}
