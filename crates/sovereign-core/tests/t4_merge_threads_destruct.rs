//! v0.0.12 item 23, **T4** — "Merging threads always asks"
//! (`doc/plans/V0012-item23-document-titles.md`; Céline, 2026-10-25).
//!
//! `merge_threads` deletes its source thread (`DocumentService::thread_merge`),
//! yet it sat at Modify, the one level trust auto-approves after five
//! approvals. It moves to Destruct, like `delete_thread`: never auto-approved,
//! whatever the trust state (the trust side is tested in
//! `sovereign-ai/src/orchestrator_document_titles_tests.rs`, `t4_…`). Nothing
//! else about the merge changes in v0.0.12.
//!
//! Written before the code by another author; the implementer does not edit
//! this file. Made stale by this contract (the contract author edits it):
//! `security.rs`'s own `action_level_merge_split_modify`.

use sovereign_core::security::{action_level, ActionLevel};

#[test]
fn t4_merge_threads_is_destruct() {
    assert_eq!(action_level("merge_threads"), ActionLevel::Destruct);
    assert_eq!(action_level("merge_threads"), action_level("delete_thread"), "the same level as deleting a thread");
}

/// T4: nothing else changes — split stays Modify.
#[test]
fn t4_split_thread_stays_modify() {
    assert_eq!(action_level("split_thread"), ActionLevel::Modify);
}
