use serde::{Deserialize, Serialize};

/// An encrypted commit for sync transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedCommit {
    /// The commit ID.
    pub commit_id: String,
    /// The document this commit belongs to.
    pub document_id: String,
    /// Parent commit ID (for chain verification).
    pub parent_commit: Option<String>,
    /// Base64-encoded encrypted commit snapshot.
    pub encrypted_snapshot: String,
    /// Base64-encoded nonce for the snapshot.
    pub nonce: String,
    /// Commit message (not encrypted — metadata).
    pub message: String,
    /// ISO-8601 timestamp.
    pub timestamp: String,
}

/// Result of comparing two manifests.
#[derive(Debug, Clone)]
pub struct SyncDiff {
    /// Documents we need from the remote.
    pub need_from_remote: Vec<String>,
    /// Documents the remote needs from us.
    pub push_to_remote: Vec<String>,
    /// Documents that diverged (both sides have changes).
    pub conflicts: Vec<SyncConflict>,
    /// Documents that are in sync.
    pub in_sync: Vec<String>,
}

/// A sync conflict between two devices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConflict {
    pub doc_id: String,
    pub local_head: Option<String>,
    pub remote_head: Option<String>,
    pub local_commit_count: u32,
    pub remote_commit_count: u32,
}

impl SyncDiff {
    /// Whether there's anything to sync.
    pub fn has_work(&self) -> bool {
        !self.need_from_remote.is_empty()
            || !self.push_to_remote.is_empty()
            || !self.conflicts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_commit_serde() {
        let commit = EncryptedCommit {
            commit_id: "commit:abc".into(),
            document_id: "document:123".into(),
            parent_commit: None,
            encrypted_snapshot: "base64data".into(),
            nonce: "base64nonce".into(),
            message: "initial".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&commit).unwrap();
        let back: EncryptedCommit = serde_json::from_str(&json).unwrap();
        assert_eq!(back.commit_id, "commit:abc");
    }

    #[test]
    fn sync_diff_has_work() {
        let empty = SyncDiff {
            need_from_remote: vec![],
            push_to_remote: vec![],
            conflicts: vec![],
            in_sync: vec!["doc:1".into()],
        };
        assert!(!empty.has_work());

        let with_need = SyncDiff {
            need_from_remote: vec!["doc:2".into()],
            push_to_remote: vec![],
            conflicts: vec![],
            in_sync: vec![],
        };
        assert!(with_need.has_work());
    }

    #[test]
    fn sync_conflict_serde() {
        let conflict = SyncConflict {
            doc_id: "document:abc".into(),
            local_head: Some("commit:local".into()),
            remote_head: Some("commit:remote".into()),
            local_commit_count: 5,
            remote_commit_count: 7,
        };
        let json = serde_json::to_string(&conflict).unwrap();
        let back: SyncConflict = serde_json::from_str(&json).unwrap();
        assert_eq!(back.local_commit_count, 5);
        assert_eq!(back.remote_commit_count, 7);
    }
}
