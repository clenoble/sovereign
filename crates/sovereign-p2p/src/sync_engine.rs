use std::collections::{HashMap, HashSet};

use crate::protocol::manifest::{DocumentManifestEntry, SyncManifest};
use crate::protocol::sync::{SyncConflict, SyncDiff};

/// Computes the diff between a local and remote manifest.
pub fn compute_diff(local: &SyncManifest, remote: &SyncManifest) -> SyncDiff {
    let local_map: HashMap<&str, &DocumentManifestEntry> = local
        .entries
        .iter()
        .map(|e| (e.doc_id.as_str(), e))
        .collect();

    let remote_map: HashMap<&str, &DocumentManifestEntry> = remote
        .entries
        .iter()
        .map(|e| (e.doc_id.as_str(), e))
        .collect();

    let all_ids: HashSet<&str> = local_map
        .keys()
        .chain(remote_map.keys())
        .copied()
        .collect();

    let mut need_from_remote = Vec::new();
    let mut push_to_remote = Vec::new();
    let mut conflicts = Vec::new();
    let mut in_sync = Vec::new();

    for doc_id in all_ids {
        match (local_map.get(doc_id), remote_map.get(doc_id)) {
            (None, Some(_)) => {
                // Remote has it, we don't
                need_from_remote.push(doc_id.to_string());
            }
            (Some(_), None) => {
                // We have it, remote doesn't
                push_to_remote.push(doc_id.to_string());
            }
            (Some(local_entry), Some(remote_entry)) => {
                if local_entry.content_hash == remote_entry.content_hash {
                    // Same content
                    in_sync.push(doc_id.to_string());
                } else if local_entry.head_commit == remote_entry.head_commit {
                    // Same head but different hash — shouldn't happen, treat as conflict
                    conflicts.push(SyncConflict {
                        doc_id: doc_id.to_string(),
                        local_head: local_entry.head_commit.clone(),
                        remote_head: remote_entry.head_commit.clone(),
                        local_commit_count: local_entry.commit_count,
                        remote_commit_count: remote_entry.commit_count,
                    });
                } else if local_entry.commit_count > remote_entry.commit_count
                    && is_ancestor(&remote_entry.head_commit, &local_entry.head_commit)
                {
                    // Local is ahead — push to remote
                    push_to_remote.push(doc_id.to_string());
                } else if remote_entry.commit_count > local_entry.commit_count
                    && is_ancestor(&local_entry.head_commit, &remote_entry.head_commit)
                {
                    // Remote is ahead — need from remote
                    need_from_remote.push(doc_id.to_string());
                } else {
                    // Both have diverged
                    conflicts.push(SyncConflict {
                        doc_id: doc_id.to_string(),
                        local_head: local_entry.head_commit.clone(),
                        remote_head: remote_entry.head_commit.clone(),
                        local_commit_count: local_entry.commit_count,
                        remote_commit_count: remote_entry.commit_count,
                    });
                }
            }
            (None, None) => unreachable!(),
        }
    }

    SyncDiff {
        need_from_remote,
        push_to_remote,
        conflicts,
        in_sync,
    }
}

/// Placeholder ancestry check.
/// In a real implementation, this would walk the commit chain.
/// For now, we use a heuristic: if both commits exist and differ, it's a divergence.
fn is_ancestor(potential_ancestor: &Option<String>, _descendant: &Option<String>) -> bool {
    // Without access to the commit chain, we can't determine ancestry.
    // This will be connected to the DB in Phase 3B integration.
    // For now, return false to treat unequal heads as conflicts.
    potential_ancestor.is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::manifest::DocumentManifestEntry;

    fn entry(doc_id: &str, head: Option<&str>, count: u32, hash: &str) -> DocumentManifestEntry {
        DocumentManifestEntry {
            doc_id: doc_id.into(),
            head_commit: head.map(String::from),
            commit_count: count,
            content_hash: hash.into(),
            modified_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn identical_manifests() {
        let local = SyncManifest {
            device_id: "dev-1".into(),
            generated_at: "now".into(),
            entries: vec![entry("doc:1", Some("c:1"), 1, "hash1")],
        };
        let remote = SyncManifest {
            device_id: "dev-2".into(),
            generated_at: "now".into(),
            entries: vec![entry("doc:1", Some("c:1"), 1, "hash1")],
        };
        let diff = compute_diff(&local, &remote);
        assert!(!diff.has_work());
        assert_eq!(diff.in_sync.len(), 1);
    }

    #[test]
    fn remote_has_new_doc() {
        let local = SyncManifest {
            device_id: "dev-1".into(),
            generated_at: "now".into(),
            entries: vec![],
        };
        let remote = SyncManifest {
            device_id: "dev-2".into(),
            generated_at: "now".into(),
            entries: vec![entry("doc:1", Some("c:1"), 1, "hash1")],
        };
        let diff = compute_diff(&local, &remote);
        assert_eq!(diff.need_from_remote, vec!["doc:1"]);
    }

    #[test]
    fn local_has_new_doc() {
        let local = SyncManifest {
            device_id: "dev-1".into(),
            generated_at: "now".into(),
            entries: vec![entry("doc:1", Some("c:1"), 1, "hash1")],
        };
        let remote = SyncManifest {
            device_id: "dev-2".into(),
            generated_at: "now".into(),
            entries: vec![],
        };
        let diff = compute_diff(&local, &remote);
        assert_eq!(diff.push_to_remote, vec!["doc:1"]);
    }

    #[test]
    fn diverged_docs_are_conflicts() {
        let local = SyncManifest {
            device_id: "dev-1".into(),
            generated_at: "now".into(),
            entries: vec![entry("doc:1", Some("c:local"), 3, "hash-local")],
        };
        let remote = SyncManifest {
            device_id: "dev-2".into(),
            generated_at: "now".into(),
            entries: vec![entry("doc:1", Some("c:remote"), 4, "hash-remote")],
        };
        let diff = compute_diff(&local, &remote);
        assert_eq!(diff.conflicts.len(), 1);
        assert_eq!(diff.conflicts[0].doc_id, "doc:1");
    }

    #[test]
    fn remote_ahead_from_scratch() {
        // Remote has commits, local has none (new doc pulled from remote)
        let local = SyncManifest {
            device_id: "dev-1".into(),
            generated_at: "now".into(),
            entries: vec![entry("doc:1", None, 0, "empty")],
        };
        let remote = SyncManifest {
            device_id: "dev-2".into(),
            generated_at: "now".into(),
            entries: vec![entry("doc:1", Some("c:5"), 5, "hash5")],
        };
        let diff = compute_diff(&local, &remote);
        // local head is None, is_ancestor returns true for None, so need_from_remote
        assert_eq!(diff.need_from_remote, vec!["doc:1"]);
    }

    #[test]
    fn mixed_scenario() {
        let local = SyncManifest {
            device_id: "dev-1".into(),
            generated_at: "now".into(),
            entries: vec![
                entry("doc:1", Some("c:1"), 1, "hash1"), // in sync
                entry("doc:2", Some("c:2"), 2, "hash2"), // only local
                entry("doc:3", Some("c:3a"), 3, "hashA"), // conflict
            ],
        };
        let remote = SyncManifest {
            device_id: "dev-2".into(),
            generated_at: "now".into(),
            entries: vec![
                entry("doc:1", Some("c:1"), 1, "hash1"), // in sync
                entry("doc:3", Some("c:3b"), 4, "hashB"), // conflict
                entry("doc:4", Some("c:4"), 1, "hash4"), // only remote
            ],
        };
        let diff = compute_diff(&local, &remote);
        assert_eq!(diff.in_sync, vec!["doc:1"]);
        assert!(diff.push_to_remote.contains(&"doc:2".to_string()));
        assert!(diff.need_from_remote.contains(&"doc:4".to_string()));
        assert_eq!(diff.conflicts.len(), 1);
        assert_eq!(diff.conflicts[0].doc_id, "doc:3");
    }
}
