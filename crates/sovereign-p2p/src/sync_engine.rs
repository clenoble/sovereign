use std::collections::{HashMap, HashSet};

use crate::protocol::manifest::{
    DocumentManifestEntry, EntityManifestEntry, PiiRecordManifestEntry, RowManifestEntry,
    ShareRecordManifestEntry, SyncManifest, ThreadManifestEntry,
};
use crate::protocol::sync::{SyncConflict, SyncDiff, SyncTable};

/// Per-table row diff used for the LWW row protocol. Documents have
/// their own commit-aware diff via `compute_diff` / `SyncDiff`.
#[derive(Debug, Clone, Default)]
pub struct RowDiff {
    pub need_from_remote: Vec<String>,
    pub push_to_remote: Vec<String>,
}

impl RowDiff {
    pub fn has_work(&self) -> bool {
        !self.need_from_remote.is_empty() || !self.push_to_remote.is_empty()
    }
}

/// Computes the diff between a local and remote manifest.
pub fn compute_diff(local: &SyncManifest, remote: &SyncManifest) -> SyncDiff {
    let local_map: HashMap<&str, &DocumentManifestEntry> = local
        .documents
        .iter()
        .map(|e| (e.doc_id.as_str(), e))
        .collect();

    let remote_map: HashMap<&str, &DocumentManifestEntry> = remote
        .documents
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
                // Documents sync as content-LWW on `modified_at` (the
                // plaintext-on-sync-boundary model can't replay an encrypted
                // commit chain across the per-device at-rest key boundary, so
                // commit ancestry is not used for cross-device direction).
                if local_entry.content_hash == remote_entry.content_hash {
                    in_sync.push(doc_id.to_string());
                } else {
                    match local_entry.modified_at.cmp(&remote_entry.modified_at) {
                        std::cmp::Ordering::Less => need_from_remote.push(doc_id.to_string()),
                        std::cmp::Ordering::Greater => push_to_remote.push(doc_id.to_string()),
                        std::cmp::Ordering::Equal => {
                            // P2P-002: concurrent edit — same timestamp, diverging
                            // content. Pushing from BOTH sides made the two copies
                            // swap and flap on every sync (each side overwrites the
                            // other's equal-timestamp version, forever). Break the
                            // tie deterministically on content_hash: the
                            // lexicographically greater hash wins. The comparison is
                            // symmetric (device A sees local=A/remote=B; device B
                            // sees local=B/remote=A), so both independently pick the
                            // SAME absolute winner and converge. content_hash is
                            // guaranteed to differ here (equal hashes took the
                            // in_sync branch above). Still surfaced as a conflict.
                            conflicts.push(SyncConflict {
                                doc_id: doc_id.to_string(),
                                local_head: local_entry.head_commit.clone(),
                                remote_head: remote_entry.head_commit.clone(),
                                local_commit_count: local_entry.commit_count,
                                remote_commit_count: remote_entry.commit_count,
                            });
                            if local_entry.content_hash > remote_entry.content_hash {
                                push_to_remote.push(doc_id.to_string());
                            } else {
                                need_from_remote.push(doc_id.to_string());
                            }
                        }
                    }
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

// ----- Row-level diff (non-document tables, Phase 3 v0.0.5) -----

/// Compute the row-level diff for one of the four LWW tables. Generic
/// over any manifest entry type that exposes its identity, the LWW
/// timestamp, the content hash, and an optional soft-delete marker.
///
/// Resolution rules (per row id):
/// - id only on remote → need_from_remote
/// - id only on local  → push_to_remote
/// - same content_hash → in sync (skipped)
/// - different hash, remote ts > local ts → need_from_remote
/// - different hash, local ts > remote ts → push_to_remote
/// - different hash, equal ts → both sides push (deterministic
///   tiebreak happens server-side in `SyncService::apply_rows`)
fn compute_row_diff_generic<L, R, FL, FR>(
    locals: &[L],
    remotes: &[R],
    local_id: FL,
    remote_id: FR,
    local_view: impl Fn(&L) -> (&str, &str), // (timestamp, content_hash)
    remote_view: impl Fn(&R) -> (&str, &str),
) -> RowDiff
where
    FL: Fn(&L) -> &str,
    FR: Fn(&R) -> &str,
{
    let local_map: HashMap<&str, &L> =
        locals.iter().map(|l| (local_id(l), l)).collect();
    let remote_map: HashMap<&str, &R> =
        remotes.iter().map(|r| (remote_id(r), r)).collect();

    let all_ids: HashSet<&str> = local_map
        .keys()
        .chain(remote_map.keys())
        .copied()
        .collect();

    let mut diff = RowDiff::default();
    for id in all_ids {
        match (local_map.get(id), remote_map.get(id)) {
            (None, Some(_)) => diff.need_from_remote.push(id.to_string()),
            (Some(_), None) => diff.push_to_remote.push(id.to_string()),
            (Some(l), Some(r)) => {
                let (lts, lh) = local_view(l);
                let (rts, rh) = remote_view(r);
                if lh == rh {
                    continue; // in sync
                }
                match lts.cmp(rts) {
                    std::cmp::Ordering::Less => diff.need_from_remote.push(id.to_string()),
                    std::cmp::Ordering::Greater => diff.push_to_remote.push(id.to_string()),
                    std::cmp::Ordering::Equal => {
                        // Equal timestamps with diverging hashes: push from
                        // both sides; LWW resolution in apply_rows will
                        // collapse correctly (or both sides will keep their
                        // local row as a no-op).
                        diff.need_from_remote.push(id.to_string());
                        diff.push_to_remote.push(id.to_string());
                    }
                }
            }
            (None, None) => unreachable!(),
        }
    }
    diff
}

pub fn compute_thread_diff(
    locals: &[ThreadManifestEntry],
    remotes: &[ThreadManifestEntry],
) -> RowDiff {
    compute_row_diff_generic(
        locals,
        remotes,
        |l| l.thread_id.as_str(),
        |r| r.thread_id.as_str(),
        |l| (l.modified_at.as_str(), l.content_hash.as_str()),
        |r| (r.modified_at.as_str(), r.content_hash.as_str()),
    )
}

pub fn compute_entity_diff(
    locals: &[EntityManifestEntry],
    remotes: &[EntityManifestEntry],
) -> RowDiff {
    compute_row_diff_generic(
        locals,
        remotes,
        |l| l.entity_id.as_str(),
        |r| r.entity_id.as_str(),
        |l| (l.modified_at.as_str(), l.content_hash.as_str()),
        |r| (r.modified_at.as_str(), r.content_hash.as_str()),
    )
}

pub fn compute_pii_record_diff(
    locals: &[PiiRecordManifestEntry],
    remotes: &[PiiRecordManifestEntry],
) -> RowDiff {
    compute_row_diff_generic(
        locals,
        remotes,
        |l| l.record_id.as_str(),
        |r| r.record_id.as_str(),
        |l| (l.discovered_at.as_str(), l.content_hash.as_str()),
        |r| (r.discovered_at.as_str(), r.content_hash.as_str()),
    )
}

/// Diff for any table whose manifest uses the generic `RowManifestEntry`
/// shape (the P2 tables: contacts, messages, conversations, milestones,
/// relationships, suggested links).
pub fn compute_generic_row_diff(
    locals: &[RowManifestEntry],
    remotes: &[RowManifestEntry],
) -> RowDiff {
    compute_row_diff_generic(
        locals,
        remotes,
        |l| l.id.as_str(),
        |r| r.id.as_str(),
        |l| (l.modified_at.as_str(), l.content_hash.as_str()),
        |r| (r.modified_at.as_str(), r.content_hash.as_str()),
    )
}

pub fn compute_share_record_diff(
    locals: &[ShareRecordManifestEntry],
    remotes: &[ShareRecordManifestEntry],
) -> RowDiff {
    // Share records are append-only, so timestamps don't really matter
    // for resolution — anything missing on either side is requested /
    // pushed, and identical content_hashes are skipped.
    compute_row_diff_generic(
        locals,
        remotes,
        |l| l.record_id.as_str(),
        |r| r.record_id.as_str(),
        |l| (l.shared_at.as_str(), l.content_hash.as_str()),
        |r| (r.shared_at.as_str(), r.content_hash.as_str()),
    )
}

/// Compute row diffs for every non-document table in a manifest pair.
/// Returns a map `SyncTable → RowDiff` for tables that have any work.
pub fn compute_all_row_diffs(
    local: &SyncManifest,
    remote: &SyncManifest,
) -> HashMap<SyncTable, RowDiff> {
    let mut out = HashMap::new();

    let d = compute_thread_diff(&local.threads, &remote.threads);
    if d.has_work() {
        out.insert(SyncTable::Thread, d);
    }
    let d = compute_entity_diff(&local.entities, &remote.entities);
    if d.has_work() {
        out.insert(SyncTable::Entity, d);
    }
    let d = compute_pii_record_diff(&local.pii_records, &remote.pii_records);
    if d.has_work() {
        out.insert(SyncTable::PiiRecord, d);
    }
    let d = compute_share_record_diff(&local.share_records, &remote.share_records);
    if d.has_work() {
        out.insert(SyncTable::ShareRecord, d);
    }

    // P2 tables — all use the generic entry shape.
    for (table, locals, remotes) in [
        (SyncTable::Contact, &local.contacts, &remote.contacts),
        (SyncTable::Message, &local.messages, &remote.messages),
        (SyncTable::Conversation, &local.conversations, &remote.conversations),
        (SyncTable::Milestone, &local.milestones, &remote.milestones),
        (SyncTable::Relationship, &local.relationships, &remote.relationships),
        (SyncTable::SuggestedLink, &local.suggested_links, &remote.suggested_links),
    ] {
        let d = compute_generic_row_diff(locals, remotes);
        if d.has_work() {
            out.insert(table, d);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::manifest::DocumentManifestEntry;

    fn entry(doc_id: &str, head: Option<&str>, count: u32, hash: &str) -> DocumentManifestEntry {
        entry_at(doc_id, head, count, hash, "2026-01-01T00:00:00Z")
    }

    fn entry_at(
        doc_id: &str,
        head: Option<&str>,
        count: u32,
        hash: &str,
        modified_at: &str,
    ) -> DocumentManifestEntry {
        DocumentManifestEntry {
            doc_id: doc_id.into(),
            head_commit: head.map(String::from),
            commit_count: count,
            content_hash: hash.into(),
            modified_at: modified_at.into(),
            deleted_at: None,
        }
    }

    fn manifest_with(device: &str, docs: Vec<DocumentManifestEntry>) -> SyncManifest {
        let mut m = SyncManifest::new(device.into());
        m.generated_at = "now".into();
        m.documents = docs;
        m
    }

    #[test]
    fn identical_manifests() {
        let local = manifest_with("dev-1", vec![entry("doc:1", Some("c:1"), 1, "hash1")]);
        let remote = manifest_with("dev-2", vec![entry("doc:1", Some("c:1"), 1, "hash1")]);
        let diff = compute_diff(&local, &remote);
        assert!(!diff.has_work());
        assert_eq!(diff.in_sync.len(), 1);
    }

    #[test]
    fn remote_has_new_doc() {
        let local = manifest_with("dev-1", vec![]);
        let remote = manifest_with("dev-2", vec![entry("doc:1", Some("c:1"), 1, "hash1")]);
        let diff = compute_diff(&local, &remote);
        assert_eq!(diff.need_from_remote, vec!["doc:1"]);
    }

    #[test]
    fn local_has_new_doc() {
        let local = manifest_with("dev-1", vec![entry("doc:1", Some("c:1"), 1, "hash1")]);
        let remote = manifest_with("dev-2", vec![]);
        let diff = compute_diff(&local, &remote);
        assert_eq!(diff.push_to_remote, vec!["doc:1"]);
    }

    #[test]
    fn concurrent_edit_same_timestamp_is_conflict() {
        // Same modified_at, diverging content → concurrent edit conflict.
        let local = manifest_with("dev-1", vec![entry("doc:1", Some("c:local"), 3, "hash-local")]);
        let remote = manifest_with("dev-2", vec![entry("doc:1", Some("c:remote"), 4, "hash-remote")]);
        let diff = compute_diff(&local, &remote);
        assert_eq!(diff.conflicts.len(), 1);
        assert_eq!(diff.conflicts[0].doc_id, "doc:1");
    }

    #[test]
    fn remote_newer_is_fetched() {
        // Content-LWW: remote has a strictly newer modified_at → fetch it.
        let local = manifest_with(
            "dev-1",
            vec![entry_at("doc:1", None, 0, "empty", "2026-01-01T00:00:00Z")],
        );
        let remote = manifest_with(
            "dev-2",
            vec![entry_at("doc:1", Some("c:5"), 5, "hash5", "2026-02-01T00:00:00Z")],
        );
        let diff = compute_diff(&local, &remote);
        assert_eq!(diff.need_from_remote, vec!["doc:1"]);
        assert!(diff.push_to_remote.is_empty());
        assert!(diff.conflicts.is_empty());
    }

    #[test]
    fn local_newer_is_pushed() {
        let local = manifest_with(
            "dev-1",
            vec![entry_at("doc:1", Some("c:2"), 2, "hashA", "2026-03-01T00:00:00Z")],
        );
        let remote = manifest_with(
            "dev-2",
            vec![entry_at("doc:1", Some("c:1"), 1, "hashB", "2026-01-01T00:00:00Z")],
        );
        let diff = compute_diff(&local, &remote);
        assert_eq!(diff.push_to_remote, vec!["doc:1"]);
        assert!(diff.need_from_remote.is_empty());
    }

    #[test]
    fn mixed_scenario() {
        let local = manifest_with(
            "dev-1",
            vec![
                entry("doc:1", Some("c:1"), 1, "hash1"),  // in sync
                entry("doc:2", Some("c:2"), 2, "hash2"),  // only local
                entry("doc:3", Some("c:3a"), 3, "hashA"), // concurrent-edit conflict
            ],
        );
        let remote = manifest_with(
            "dev-2",
            vec![
                entry("doc:1", Some("c:1"), 1, "hash1"),  // in sync
                entry("doc:3", Some("c:3b"), 4, "hashB"), // concurrent-edit conflict
                entry("doc:4", Some("c:4"), 1, "hash4"),  // only remote
            ],
        );
        let diff = compute_diff(&local, &remote);
        assert_eq!(diff.in_sync, vec!["doc:1"]);
        assert!(diff.push_to_remote.contains(&"doc:2".to_string()));
        assert!(diff.need_from_remote.contains(&"doc:4".to_string()));
        assert_eq!(diff.conflicts.len(), 1);
        assert_eq!(diff.conflicts[0].doc_id, "doc:3");
    }

    // ----- Row-level diff tests (LWW tables) -----

    fn thread_entry(id: &str, ts: &str, hash: &str) -> ThreadManifestEntry {
        ThreadManifestEntry {
            thread_id: id.into(),
            modified_at: ts.into(),
            content_hash: hash.into(),
            deleted_at: None,
        }
    }

    fn pii_entry(id: &str, ts: &str, hash: &str) -> PiiRecordManifestEntry {
        PiiRecordManifestEntry {
            record_id: id.into(),
            discovered_at: ts.into(),
            content_hash: hash.into(),
            deleted_at: None,
        }
    }

    #[test]
    fn row_diff_only_remote_needs_fetch() {
        let diff = compute_thread_diff(
            &[],
            &[thread_entry("thread:1", "2026-01-01T00:00:00Z", "hash1")],
        );
        assert_eq!(diff.need_from_remote, vec!["thread:1"]);
        assert!(diff.push_to_remote.is_empty());
    }

    #[test]
    fn row_diff_only_local_needs_push() {
        let diff = compute_thread_diff(
            &[thread_entry("thread:1", "2026-01-01T00:00:00Z", "hash1")],
            &[],
        );
        assert_eq!(diff.push_to_remote, vec!["thread:1"]);
    }

    #[test]
    fn row_diff_same_hash_is_no_op() {
        let diff = compute_thread_diff(
            &[thread_entry("thread:1", "2026-01-01T00:00:00Z", "hash1")],
            &[thread_entry("thread:1", "2026-01-02T00:00:00Z", "hash1")], // ts differs but hash equal
        );
        assert!(!diff.has_work());
    }

    #[test]
    fn row_diff_remote_newer_needs_fetch() {
        let diff = compute_thread_diff(
            &[thread_entry("thread:1", "2026-01-01T00:00:00Z", "hashA")],
            &[thread_entry("thread:1", "2026-02-01T00:00:00Z", "hashB")],
        );
        assert_eq!(diff.need_from_remote, vec!["thread:1"]);
        assert!(diff.push_to_remote.is_empty());
    }

    #[test]
    fn row_diff_local_newer_pushes() {
        let diff = compute_thread_diff(
            &[thread_entry("thread:1", "2026-02-01T00:00:00Z", "hashA")],
            &[thread_entry("thread:1", "2026-01-01T00:00:00Z", "hashB")],
        );
        assert_eq!(diff.push_to_remote, vec!["thread:1"]);
        assert!(diff.need_from_remote.is_empty());
    }

    #[test]
    fn row_diff_equal_ts_diverging_hash_pushes_both() {
        let diff = compute_thread_diff(
            &[thread_entry("thread:1", "2026-01-01T00:00:00Z", "hashA")],
            &[thread_entry("thread:1", "2026-01-01T00:00:00Z", "hashB")],
        );
        assert_eq!(diff.need_from_remote.len(), 1);
        assert_eq!(diff.push_to_remote.len(), 1);
    }

    #[test]
    fn pii_diff_uses_discovered_at() {
        let diff = compute_pii_record_diff(
            &[pii_entry("pii_record:1", "2026-01-01T00:00:00Z", "hashA")],
            &[pii_entry("pii_record:1", "2026-02-01T00:00:00Z", "hashB")],
        );
        assert_eq!(diff.need_from_remote, vec!["pii_record:1"]);
    }

    #[test]
    fn compute_all_row_diffs_only_returns_tables_with_work() {
        let mut local = manifest_with("dev-1", vec![]);
        let mut remote = manifest_with("dev-2", vec![]);
        local.threads.push(thread_entry("thread:1", "2026-01-01T00:00:00Z", "hashA"));
        remote.threads.push(thread_entry("thread:1", "2026-02-01T00:00:00Z", "hashB"));
        // entities, pii, share are all empty → no work expected

        let diffs = compute_all_row_diffs(&local, &remote);
        assert_eq!(diffs.len(), 1);
        assert!(diffs.contains_key(&SyncTable::Thread));
    }
}
