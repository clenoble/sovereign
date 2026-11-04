//! v0.0.12 lanes lot, item 15 — **M2**: every move goes through the one
//! public move (M1). Contract: `doc/plans/V0012-lot-lanes.md`.
//!
//! "The shell's drag, the assistant's move (orchestrator and tool), the
//! restore into a thread, and the Tauri command all move through M1. The
//! restore into a thread becomes one service operation under one lock … A
//! source scan (as `ts1_one_thread_write_path.rs` does for threads) fails if
//! production code outside `sovereign-db` calls `GraphDB::move_document_to_thread`."
//!
//! Here: that scan; that the restore host and the Tauri command reach the
//! service; and the host's restore as one stamped state (it compiles today).
//! The callers' behaviour is tested where they live: the shell's drag in
//! `sovereign-shell/src/lanes_tests.rs`, the assistant in
//! `sovereign-ai/src/orchestrator_lanes_tests.rs`. The Tauri app is read, not
//! run (as in `ts1`).
//!
//! Written before the code, by another author; the implementer does not edit
//! this file. API named: `DocumentService::move_to_thread`,
//! `DocumentService::restore_from_trash_into` (see `lanes_m1_move.rs`).

mod common;

use chrono::Duration;
use common::scan::{calls, crate_dirs, production_files, rust_files};
use common::wire::*;
use common::*;
use sovereign_docs::{delete_thread_to_trash, restore_to_thread};

/// The store types a path-qualified call can name.
const STORE_TYPES: [&str; 4] = ["GraphDB", "EncryptedGraphDB", "SurrealGraphDB", "LayeredGraphDB"];

/// Lines where `name` is called as a store method in `prod`: every `.name(`,
/// and `::name(` when qualified by a store type (`GraphDB::name(db, …)`). A
/// function of the same name called through its module path (the Tauri
/// command `tauri_commands::threads::move_document_to_thread`) is not a store
/// call.
fn store_calls(prod: &str, name: &str) -> Vec<usize> {
    let lines: Vec<&str> = prod.lines().collect();
    calls(prod, name)
        .into_iter()
        .filter(|&ln| {
            let line = lines[ln - 1];
            let mut from = 0;
            while let Some(rel) = line[from..].find(name) {
                let at = from + rel;
                from = at + name.len();
                if !line[at + name.len()..].trim_start().starts_with('(') {
                    continue;
                }
                let before = line[..at].trim_end();
                if before.ends_with('.') {
                    return true;
                }
                if let Some(q) = before.strip_suffix("::") {
                    let seg: String = q
                        .chars()
                        .rev()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect();
                    if STORE_TYPES.contains(&seg.as_str()) {
                        return true;
                    }
                }
            }
            false
        })
        .collect()
}

/// Test-only files declared in a form the shared scan does not read (as in
/// `ts1_one_thread_write_path.rs`).
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

/// `(crate-relative path, production text)` of every production file of the
/// workspace except `sovereign-db`'s.
fn production_outside_the_store() -> Vec<(String, String)> {
    let mut out = vec![];
    for dir in crate_dirs() {
        if dir.file_name().is_some_and(|n| n == "sovereign-db") {
            continue;
        }
        let test_only = more_test_only_files(&dir);
        for (file, prod) in production_files(&dir) {
            if test_only.iter().any(|t| *t == file) {
                continue;
            }
            let rel = file.strip_prefix(dir.parent().unwrap()).unwrap_or(&file).to_string_lossy().replace('\\', "/");
            out.push((rel, prod));
        }
    }
    out
}

fn production_of(rel: &str) -> String {
    production_outside_the_store()
        .into_iter()
        .find(|(r, _)| r == rel)
        .unwrap_or_else(|| panic!("{rel} not found among the production files"))
        .1
}

/// The text of `fn <name>(` up to the next top-level `fn` (or the end).
fn fn_text<'a>(prod: &'a str, name: &str) -> &'a str {
    let sig = format!("fn {name}(");
    let at = prod.find(&sig).unwrap_or_else(|| panic!("fn {name} not found"));
    let rest = &prod[at + sig.len()..];
    let end = ["\npub async fn ", "\npub fn ", "\nasync fn ", "\nfn ", "\n#[tauri::command]"]
        .iter()
        .filter_map(|m| rest.find(m))
        .min()
        .unwrap_or(rest.len());
    &rest[..end]
}

// ------------------------------------------------------------------ scan --

/// M2: "A source scan … fails if production code outside `sovereign-db`
/// calls `GraphDB::move_document_to_thread`." No exception: the shell's drag,
/// the assistant (orchestrator and tool), the restore host and the Tauri
/// command (compiled into the phone: its "Move to Thread") all move through M1.
#[test]
fn m2_no_production_code_moves_a_document_around_the_one_move() {
    let files = production_outside_the_store();
    assert!(files.len() > 100, "only {} production files scanned", files.len());
    let mut found = vec![];
    for (rel, prod) in &files {
        for line in store_calls(prod, "move_document_to_thread") {
            found.push(format!("{rel}:{line}"));
        }
    }
    assert!(found.is_empty(), "store moves around the document service's move (M1):\n{}", found.join("\n"));
}

/// M2: the restore into a thread is one service operation — the host asks
/// the service for it, and moves nothing itself.
#[test]
fn m2_the_restore_host_restores_into_a_thread_through_one_service_operation() {
    let prod = production_of("sovereign-docs/src/hosts.rs");
    let body = fn_text(&prod, "restore_to_thread");
    assert!(
        !calls(body, "restore_from_trash_into").is_empty(),
        "restore_to_thread restores into a thread through DocumentService::restore_from_trash_into:\n{body}"
    );
    assert!(store_calls(body, "move_document_to_thread").is_empty(), "and moves nothing around it");
}

/// M2: the Tauri command (the phone's "Move to Thread") moves through M1.
#[test]
fn m2_the_tauri_command_moves_through_the_document_service() {
    let prod = production_of("sovereign-app/src/tauri_commands/threads.rs");
    let body = fn_text(&prod, "move_document_to_thread");
    assert!(!calls(body, "move_to_thread").is_empty(), "the command calls DocumentService::move_to_thread:\n{body}");
}

// ------------------------------------------------- the host, behaviour --

/// M2 through the host the shell and the phone call: restoring into a chosen
/// thread leaves one stamped state replacing the trashed one (a move written
/// around the service, then a restore, would be two writes, or one unstamped).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m2_the_host_restore_into_a_chosen_thread_is_one_stamped_state() {
    let s = Stack::new("m2-host-restore").await;
    let old = s.svc.thread_create("Old", "").await.unwrap();
    let keep = s.svc.thread_create("Keep", "").await.unwrap();
    let mut d = new_text_doc("Draft", "draft");
    d.thread_id = old.clone();
    let id = s.svc.create(d).await.unwrap().id;
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &old).await.unwrap();
    s.clock.advance(Duration::minutes(1));
    let trashed = current_stamp(&s, &id).await;

    restore_to_thread(&s.svc, s.edb.as_ref(), &id, Some(&keep)).await.expect("restored");
    let st = s.svc.stored_state(&id).await.unwrap();
    assert_eq!(st.document.thread_id, keep, "in the chosen thread");
    assert!(st.document.deleted_at.is_none(), "out of the trash");
    let stamp = st.meta.stamp.clone().unwrap();
    assert_eq!((stamp.wall_ms, stamp.device.as_str()), (s.clock.get().timestamp_millis(), DEVICE), "stamped here, now");
    assert_eq!(st.meta.prev_stamp, Some(trashed), "one state replaces the trashed one");
}
