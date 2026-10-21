//! S5-1 — one way to delete (C-DOC adoption step 5,
//! `doc/plans/C-DOC-adoption.md`).
//!
//! "No production code outside `sovereign-db` and `sovereign-docs` calls
//! `soft_delete_document`, `delete_document`, `purge_deleted` or
//! `restore_soft_deleted_document` (a source scan, like a lint, failing with
//! the file and line)."
//!
//! Spec ch. 03, G13 ("Delete is trash") and *Soft delete and 30-day purge*
//! (decided 2026-10-10: "delete is a soft delete everywhere, with a trash").
//!
//! The first test is S5-1 as written (document functions). The thread
//! functions were out of it until Céline's answer to Q2 (2026-10-17); the
//! second test (S5-10) covers them, with sync's own exception. The action name
//! `"delete_document"` (a string in the action gate and the orchestrator) is
//! not a call, nor is a Tauri command *defined* under that name.
//! How the scan reads code: `tests/common/scan.rs`.

mod common;

use common::scan::{calls, crate_dirs, production, production_files};

const FORBIDDEN: [&str; 4] = ["soft_delete_document", "delete_document", "purge_deleted", "restore_soft_deleted_document"];

const ALLOWED_CRATES: [&str; 2] = ["sovereign-db", "sovereign-docs"];

#[test]
fn s5_1_no_production_code_outside_db_and_docs_deletes_around_the_service() {
    let mut found = vec![];
    let mut scanned = 0;
    for dir in crate_dirs() {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        if ALLOWED_CRATES.contains(&name.as_str()) {
            continue;
        }
        for (file, prod) in production_files(&dir) {
            scanned += 1;
            for f in FORBIDDEN {
                for line in calls(&prod, f) {
                    let rel = file.strip_prefix(dir.parent().unwrap()).unwrap_or(&file);
                    found.push(format!("{}:{line}: {f}", rel.display()));
                }
            }
        }
    }
    // The scan saw the workspace (a wrong path would pass vacuously).
    assert!(scanned > 100, "only {scanned} production files scanned");
    assert!(found.is_empty(), "deletes around the document service:\n{}", found.join("\n"));
}

/// S5-10 extends the rule to threads (Céline, 2026-10-17: a deleted thread's
/// documents go to the trash and the thread never comes back; plan, S5-10: "no
/// host restores a thread any more (`restore_soft_deleted_thread` stays for
/// sync only)").
///
/// - Deleting a thread (`soft_delete_thread`, or the store's hard
///   `delete_thread`, which would lose the tombstone sync needs) only in
///   `sovereign-db`, `sovereign-docs` (home of `delete_thread_to_trash`, the
///   one host path) — and in `sovereign-p2p`: sync applies a deletion that
///   arrives from a paired device to the thread row (its documents arrive with
///   their own tombstones).
/// - Restoring a thread (`restore_soft_deleted_thread`) only in `sovereign-db`
///   and `sovereign-p2p` (sync).
#[test]
fn s5_10_no_host_deletes_or_restores_a_thread_around_the_one_path() {
    let rules: [(&str, &[&str]); 3] = [
        ("soft_delete_thread", &["sovereign-db", "sovereign-docs", "sovereign-p2p"]),
        ("delete_thread", &["sovereign-db", "sovereign-docs", "sovereign-p2p"]),
        ("restore_soft_deleted_thread", &["sovereign-db", "sovereign-p2p"]),
    ];
    let mut found = vec![];
    for dir in crate_dirs() {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        for (file, prod) in production_files(&dir) {
            for (f, allowed) in rules {
                if allowed.contains(&name.as_str()) {
                    continue;
                }
                for line in calls(&prod, f) {
                    let rel = file.strip_prefix(dir.parent().unwrap()).unwrap_or(&file);
                    found.push(format!("{}:{line}: {f}", rel.display()));
                }
            }
        }
    }
    assert!(found.is_empty(), "thread deletes or restores around the one path:\n{}", found.join("\n"));
}

/// The scan itself: it sees calls, and only calls. Without this a scan that
/// matched nothing would pass S5-1 vacuously.
#[test]
fn s5_1_the_scan_sees_calls_and_only_calls() {
    let src = r#####"
use x::y;
async fn delete_document(id: String) {}            // a definition: not a call
const NAME: &str = "delete_document";             // an action name: not a call
const PAT: &str = "db.soft_delete_document(";     // inside a string: not a call
// db.delete_document(&id) in a comment: not a call
/* db.purge_deleted(age) in a block comment */
fn prod(db: &dyn GraphDB) {
    db.soft_delete_document(&id);                 // line 9: a call
    GraphDB::delete_document(db, &id);            // line 10: a call
    state
        .db
        .restore_soft_deleted_document (&id);     // line 13: a call
    let c = '{';
}
#[cfg(test)]
fn helper(db: &dyn GraphDB) { db.purge_deleted(age); }
#[cfg(test)]
mod tests {
    fn t() { db.delete_document(&id); let s = "}"; }
}
#[cfg(test)]
mod step5_tests;
fn after_tests(db: &dyn GraphDB) {
    db.purge_deleted(age);                        // line 25: a call after the test module
}
"#####;
    let (prod, test_mods) = production(src);
    let mut seen = vec![];
    for f in FORBIDDEN {
        for l in calls(&prod, f) {
            seen.push((l, f));
        }
    }
    seen.sort();
    assert_eq!(
        seen,
        vec![
            (9, "soft_delete_document"),
            (10, "delete_document"),
            (13, "restore_soft_deleted_document"),
            (25, "purge_deleted"),
        ]
    );
    assert_eq!(test_mods, vec!["step5_tests".to_string()]);
}
