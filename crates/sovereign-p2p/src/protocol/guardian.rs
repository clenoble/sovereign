use serde::{Deserialize, Serialize};

/// Request to deliver a shard to a guardian.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardDeliveryRequest {
    /// The shard data (base64-encoded).
    pub shard_data: String,
    /// Shard ID for tracking.
    pub shard_id: String,
    /// User the shard belongs to.
    pub for_user: String,
    /// Key rotation epoch.
    pub epoch: u32,
}

/// Request a shard back during recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardRecoveryRequest {
    /// Recovery request ID.
    pub request_id: String,
    /// User requesting recovery.
    pub for_user: String,
    /// The epoch of the shard being requested.
    pub epoch: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_delivery_serde() {
        let req = ShardDeliveryRequest {
            shard_data: "base64shard".into(),
            shard_id: "shard-1".into(),
            for_user: "user-1".into(),
            epoch: 1,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: ShardDeliveryRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.shard_id, "shard-1");
    }

    #[test]
    fn shard_recovery_serde() {
        let req = ShardRecoveryRequest {
            request_id: "recovery-1".into(),
            for_user: "user-1".into(),
            epoch: 2,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: ShardRecoveryRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.epoch, 2);
    }
}
