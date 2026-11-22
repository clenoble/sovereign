//! v0.0.13 lot L5, B1 — **C-MERGE MG6** (spec ch. 03, v2.0.109): « Level 3
//! (Modify) (Céline, 2026-11-13: « Niveau 3 normal »). A merge is reversible,
//! so it leaves level 5. » This replaces the rule of v0.0.12, « merging always
//! asks », which held while a merge deleted its source (v0.0.12 item 23, T4;
//! Céline, 2026-10-25): that file's `t4_merge_threads_is_destruct` is retired,
//! contradicted by MG6; its `t4_split_thread_stays_modify` still holds and is
//! kept below.
//!
//! The trust side (earned trust, the counts of v0.0.12 that do not count) is
//! in `sovereign-ai/src/orchestrator_b1_merge_tests.rs`.
//!
//! Written before the code by the B1 test author; the implementer does not
//! edit this file.

use sovereign_core::security::{action_level, ActionLevel};

/// MG6: a merge is at level 3, as a rename or a move is, and no longer at
/// the level of a thread's deletion.
///
/// Red today: `merge_threads` maps to Destruct (v0.0.12, T4).
#[test]
fn mg6_merge_threads_is_modify() {
    assert_eq!(action_level("merge_threads"), ActionLevel::Modify);
    assert_eq!(action_level("merge_threads"), action_level("rename_thread"), "the level of a rename");
    assert_ne!(action_level("merge_threads"), action_level("delete_thread"), "no longer the level of a deletion");
}

/// Kept from v0.0.12's T4: split stays Modify.
#[test]
fn t4_split_thread_stays_modify() {
    assert_eq!(action_level("split_thread"), ActionLevel::Modify);
}
