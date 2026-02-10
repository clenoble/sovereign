use serde::{Deserialize, Serialize};

/// A Guardian-held shard of the user's master key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shard {
    /// Unique identifier for this shard.
    pub shard_id: String,
    /// The Shamir share data, base64-encoded.
    pub encrypted_data: String,
    /// The user this shard belongs to.
    pub for_user: String,
    /// Fingerprint of the guardian's public key.
    pub guardian_pubkey_fingerprint: String,
    /// ISO-8601 timestamp of shard creation.
    pub created_at: String,
    /// Key rotation epoch this shard belongs to.
    pub epoch: u32,
}

/// Information about a registered Guardian.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianInfo {
    /// Unique identifier for this guardian.
    pub guardian_id: String,
    /// Display name.
    pub name: String,
    /// Contact method.
    pub contact: GuardianContact,
    /// Current status.
    pub status: GuardianStatus,
    /// ISO-8601 timestamp of enrollment.
    pub enrolled_at: String,
    /// Peer ID in the P2P network (if known).
    pub peer_id: Option<String>,
}

/// How to reach a guardian.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GuardianContact {
    /// Direct P2P connection.
    PeerId(String),
    /// Manual (out-of-band) shard exchange.
    Manual { description: String },
}

/// Guardian's current standing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuardianStatus {
    Active,
    Pending,
    Revoked,
    Unresponsive,
}

/// Persisted guardian registry, encrypted at rest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardianRegistry {
    pub guardians: Vec<GuardianInfo>,
    pub shards: Vec<Shard>,
}

impl GuardianRegistry {
    pub fn new() -> Self {
        Self {
            guardians: Vec::new(),
            shards: Vec::new(),
        }
    }

    pub fn add_guardian(&mut self, guardian: GuardianInfo) {
        self.guardians.push(guardian);
    }

    pub fn remove_guardian(&mut self, guardian_id: &str) -> Option<GuardianInfo> {
        if let Some(pos) = self
            .guardians
            .iter()
            .position(|g| g.guardian_id == guardian_id)
        {
            Some(self.guardians.remove(pos))
        } else {
            None
        }
    }

    pub fn get_guardian(&self, guardian_id: &str) -> Option<&GuardianInfo> {
        self.guardians.iter().find(|g| g.guardian_id == guardian_id)
    }

    pub fn active_guardians(&self) -> Vec<&GuardianInfo> {
        self.guardians
            .iter()
            .filter(|g| g.status == GuardianStatus::Active)
            .collect()
    }

    pub fn add_shard(&mut self, shard: Shard) {
        self.shards.push(shard);
    }

    pub fn shards_for_epoch(&self, epoch: u32) -> Vec<&Shard> {
        self.shards.iter().filter(|s| s.epoch == epoch).collect()
    }
}

impl Default for GuardianRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_guardian(id: &str, name: &str) -> GuardianInfo {
        GuardianInfo {
            guardian_id: id.to_string(),
            name: name.to_string(),
            contact: GuardianContact::Manual {
                description: "call them".into(),
            },
            status: GuardianStatus::Active,
            enrolled_at: "2026-01-01T00:00:00Z".into(),
            peer_id: None,
        }
    }

    #[test]
    fn registry_add_remove() {
        let mut reg = GuardianRegistry::new();
        reg.add_guardian(make_guardian("g1", "Alice"));
        reg.add_guardian(make_guardian("g2", "Bob"));
        assert_eq!(reg.guardians.len(), 2);

        let removed = reg.remove_guardian("g1").unwrap();
        assert_eq!(removed.name, "Alice");
        assert_eq!(reg.guardians.len(), 1);
    }

    #[test]
    fn active_guardians_filter() {
        let mut reg = GuardianRegistry::new();
        reg.add_guardian(make_guardian("g1", "Alice"));
        let mut revoked = make_guardian("g2", "Bob");
        revoked.status = GuardianStatus::Revoked;
        reg.add_guardian(revoked);

        let active = reg.active_guardians();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "Alice");
    }

    #[test]
    fn shard_epoch_filter() {
        let mut reg = GuardianRegistry::new();
        reg.add_shard(Shard {
            shard_id: "s1".into(),
            encrypted_data: "data1".into(),
            for_user: "user1".into(),
            guardian_pubkey_fingerprint: "fp1".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            epoch: 1,
        });
        reg.add_shard(Shard {
            shard_id: "s2".into(),
            encrypted_data: "data2".into(),
            for_user: "user1".into(),
            guardian_pubkey_fingerprint: "fp2".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            epoch: 2,
        });

        assert_eq!(reg.shards_for_epoch(1).len(), 1);
        assert_eq!(reg.shards_for_epoch(2).len(), 1);
        assert_eq!(reg.shards_for_epoch(3).len(), 0);
    }

    #[test]
    fn guardian_registry_serde_roundtrip() {
        let mut reg = GuardianRegistry::new();
        reg.add_guardian(make_guardian("g1", "Alice"));
        let json = serde_json::to_string(&reg).unwrap();
        let back: GuardianRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(back.guardians.len(), 1);
        assert_eq!(back.guardians[0].name, "Alice");
    }
}
