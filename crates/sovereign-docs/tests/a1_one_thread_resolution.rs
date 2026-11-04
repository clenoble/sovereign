//! v0.0.12 item 6, **A1** — "Every caller listed above resolves this way: one
//! resolution function, and a scan that no production caller uses the
//! single-match lookup." Contract: `doc/plans/V0012-lot-assistant.md`.
//!
//! `GraphDB::find_thread_by_name` (and the `…_token_hashes` form under it)
//! returns one blind-index match, `LIMIT 1`, with no preference for the exact
//! name: "Work" can resolve to "Work Archive" (cloud review v0.0.11,
//! finding 7). The callers the contract lists are the assistant's: the
//! orchestrator's thread actions, the tool layer and `docs_host` — all in
//! `sovereign-ai`. Their behaviour is tested in
//! `sovereign-ai/src/orchestrator_thread_names_tests.rs`; here, the scan.
//!
//! Settled (contract, "Readings settled with the test author", 1): A1 reaches
//! **every caller**, not only the assistant — also the Tauri browser's "Web"
//! thread (`sovereign-app/src/tauri_commands/browser.rs`), the bulk import's
//! folder threads (`sovereign-import/src/lib.rs`) and the skills' store bridge
//! (`sovereign-skills/src/db_bridge.rs`). The scan covers every crate's
//! production code except `sovereign-db`, where the lookup is defined and
//! implemented. Behaviour: the import in
//! `sovereign-import/tests/a1_exact_thread_names.rs`, the bridge in
//! `orchestrator_thread_names_tests.rs` (`a1_skills_bridge_finds_only_the_exact_name`).
//!
//! Written before the code, by another author; the implementer does not edit
//! this file.

mod common;

use common::scan::{calls, crate_dirs, production_files, rust_files};

/// The single-match lookups.
const SINGLE_MATCH: [&str; 2] = ["find_thread_by_name", "find_thread_by_name_token_hashes"];

/// Test-only files declared in a form the shared scan does not read (as in
/// `lanes_m2_one_move_path.rs`).
fn more_test_only_files(crate_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for f in rust_files(&crate_dir.join("src")) {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        for (i, l) in lines.iter().enumerate() {
            if !(l.starts_with("#[cfg(") && l.contains("test")) {
                continue;
            }
            let mut path_attr: Option<String> = None;
            for next in lines.iter().skip(i + 1).take(3) {
                if let Some(p) = next.strip_prefix("#[path = \"").and_then(|r| r.strip_suffix("\"]")) {
                    path_attr = Some(p.to_string());
                    continue;
                }
                if let Some(m) = next.strip_prefix("mod ").and_then(|r| r.strip_suffix(';')) {
                    let dir = f.parent().unwrap();
                    let stem = f.file_stem().unwrap().to_string_lossy().to_string();
                    let base = if ["lib", "main", "mod"].contains(&stem.as_str()) { dir.to_path_buf() } else { dir.join(&stem) };
                    out.push(match &path_attr {
                        Some(p) => dir.join(p),
                        None => base.join(format!("{}.rs", m.trim())),
                    });
                }
                break;
            }
        }
    }
    out
}

/// `(crate-relative path, line)` of every production call of a single-match
/// lookup in the crate `dir`.
fn single_match_calls(dir: &std::path::Path) -> Vec<String> {
    let test_only = more_test_only_files(dir);
    let mut found = vec![];
    for (file, prod) in production_files(dir) {
        if test_only.iter().any(|t| *t == file) {
            continue;
        }
        let rel = file.strip_prefix(dir.parent().unwrap()).unwrap_or(&file).to_string_lossy().replace('\\', "/");
        for name in SINGLE_MATCH {
            for line in calls(&prod, name) {
                found.push(format!("{rel}:{line} ({name})"));
            }
        }
    }
    found
}

/// A1: no production code of the assistant (`sovereign-ai`) calls the
/// single-match lookup; thread names go through the one resolution function,
/// which takes every candidate of the blind index and compares their names.
#[test]
fn a1_no_assistant_caller_uses_the_single_match_thread_lookup() {
    let ai = crate_dirs().into_iter().find(|d| d.ends_with("sovereign-ai")).expect("sovereign-ai");
    let files = production_files(&ai);
    assert!(files.len() > 20, "only {} production files scanned in sovereign-ai", files.len());
    let found = single_match_calls(&ai);
    assert!(
        found.is_empty(),
        "single-match thread lookups in the assistant (A1: one resolution function, exact names):\n{}",
        found.join("\n")
    );
}

/// A1, every caller: no production code of any crate except `sovereign-db`
/// calls the single-match lookup — the assistant, the browser's save, the
/// import and the skills' bridge all resolve through the one exact-name
/// function.
#[test]
fn a1_no_production_caller_in_any_crate_uses_the_single_match_thread_lookup() {
    let dirs = crate_dirs();
    assert!(dirs.len() > 10, "only {} crates found", dirs.len());
    let mut found = vec![];
    for dir in dirs {
        if dir.file_name().is_some_and(|n| n == "sovereign-db") {
            continue;
        }
        found.extend(single_match_calls(&dir));
    }
    assert!(
        found.is_empty(),
        "single-match thread lookups in production code (A1: one exact-name resolution, every caller):\n{}",
        found.join("\n")
    );
}
