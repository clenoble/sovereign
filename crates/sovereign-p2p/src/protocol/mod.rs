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
    /// Guardian shard delivery.
    DeliverShard(guardian::ShardDeliveryRequest),
    /// Request a shard for recovery.
    RequestShard(guardian::ShardRecoveryRequest),
    /// Pairing initiation.
    PairRequest { device_name: String, challenge: Vec<u8> },
    /// Pairing response.
    PairResponse { device_name: String, response: Vec<u8> },
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
    /// Shard delivery acknowledgement.
    ShardAck { accepted: bool },
    /// Shard for recovery.
    ShardData { shard_data: Option<String> },
    /// Error message.
    Error { message: String },
    /// Pairing accepted.
    PairAccepted { device_name: String, response: Vec<u8> },
    /// Pairing rejected.
    PairRejected { reason: String },
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
