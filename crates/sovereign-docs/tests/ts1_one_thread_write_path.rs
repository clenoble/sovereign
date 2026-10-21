//! THREAD-sync S1 — every thread write is stamped (`doc/plans/THREAD-sync.md`,
//! decided by Céline 2026-10-18; spec ch. 03, *Thread deletion and the
//! fallback lane*, "every thread write (create, rename, delete) carries a
//! clock stamp for this").
//!
//! "Creating, renaming (from the lane label, the assistant, or any other path)
//! and deleting a thread go through **one** thread-write path that sets an HLC
//! stamp from the same device clock as the documents' (C-DOC G25), and the row
//! carries it on the wire (a field added with a default, C-WIRE's rule for new
//! fields)."
//!
//! Here: the one path (a source scan), and the stamps at the service. The wire
//! half (the row carries the stamp, sealed; the receiver keeps it and its clock
//! moves past it) is in `sovereign-p2p/tests/wire_thread_sync.rs`.
//!
//! Index and red reasons: `crates/sovereign-p2p/TESTS-thread-sync.md`.

mod common;

use chrono::Duration;
use common::scan::{calls, crate_dirs, production_files};
use common::wire::*;
use common::*;
use sovereign_db::schema::Thread;
use sovereign_docs::{delete_thread_to_trash, OrphanResolution, RemoteDocument, Stamp};

// -------------------------------------------------------------- one path --

/// Every store call that writes a thread record.
const THREAD_WRITERS: [&str; 9] = [
    "create_thread",
    "create_thread_with_id",
    "update_thread",
    "soft_delete_thread",
    "delete_thread",
    "restore_soft_deleted_thread",
    "merge_threads",
    "split_thread",
    "set_thread_encryption",
];

/// The one thread-write path (the stub's file, `sovereign-docs/src/threads.rs`).
const THE_PATH: &str = "sovereign-docs/src/threads.rs";

/// Exceptions: `(file, writer, reason)`. S1 as settled (2026-10-18, "every live
/// path"): every production path that writes a thread — backup restore included
/// — goes through the stamped path, or does not write; only frozen Tauri
/// *desktop*-only code not compiled into the phone is out of scope. **None
/// today**: backup restore (`sovereign-p2p/src/backup.rs`) is no longer
/// exempt, and the Tauri thread writers (`tauri_commands/threads.rs`
/// `create_thread` / `update_thread`, `tauri_commands/browser.rs`
/// `save_web_page`'s "Web" thread) are all compiled into the phone (no
/// `target_os` gate, registered unconditionally in `lib.rs`), so they are live.
const EXCEPTIONS: [(&str, &str, &str); 0] = [];

/// The store types a path-qualified call can name (`GraphDB::create_thread(self,
/// …)`). A `::name(` call qualified by anything else (the CLI's own
/// `commands::create_thread`, a host function) is not a store write; a
/// method call `.name(` always counts.
const STORE_TYPES: [&str; 4] = ["GraphDB", "EncryptedGraphDB", "SurrealGraphDB", "LayeredGraphDB"];

/// Lines where `name` is called as a store write in `prod`: every `.name(`,
/// and `::name(` when qualified by a store type.
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
                    let seg: String =
                        q.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_').collect::<String>().chars().rev().collect();
                    if STORE_TYPES.contains(&seg.as_str()) {
                        return true;
                    }
                }
            }
            false
        })
        .collect()
}

/// Files of `crate_dir/src` declared as test-only modules in a form the shared
/// scan does not read: `#[cfg(test)]` or `#[cfg(all(test, …))]` followed by
/// `#[path = "…"]` and/or `mod name;` (e.g. `orchestrator_step5_tests.rs`,
/// the shell's `title_tests.rs`).
fn more_test_only_files(crate_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for f in common::scan::rust_files(&crate_dir.join("src")) {
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

/// S1: no production code outside `sovereign-db` (the store) and the one path
/// writes a thread record — not the assistant (`sovereign-ai`: its
/// `create_thread` / `rename_thread` tool and intents, merge and split), not a
/// host (shell, phone, CLI, import, seed, decoy workspace, skills bridge), not
/// `delete_thread_to_trash` or the orphan resolution outside the path, not
/// sync (applying a received row, restoring a row's previous value), not
/// backup restore (S1 as settled 2026-10-18: "every live path"; a path with no
/// document service obtains one or does not write).
#[test]
fn s1_no_production_code_writes_a_thread_around_the_one_path() {
    let mut found = vec![];
    let mut scanned = 0;
    let mut path_seen = false;
    for dir in crate_dirs() {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        if name == "sovereign-db" {
            continue;
        }
        let test_only = more_test_only_files(&dir);
        for (file, prod) in production_files(&dir) {
            if test_only.iter().any(|t| *t == file) {
                continue;
            }
            scanned += 1;
            let rel = file.strip_prefix(dir.parent().unwrap()).unwrap_or(&file);
            let rel = rel.to_string_lossy().replace('\\', "/");
            if rel == THE_PATH {
                path_seen = true;
                continue;
            }
            for w in THREAD_WRITERS {
                if EXCEPTIONS.iter().any(|(f, x, _why)| *f == rel && *x == w) {
                    continue;
                }
                for line in store_calls(&prod, w) {
                    found.push(format!("{rel}:{line}: {w}"));
                }
            }
        }
    }
    assert!(scanned > 100, "only {scanned} production files scanned");
    assert!(path_seen, "the one thread-write path {THE_PATH} was not found");
    assert!(found.is_empty(), "thread writes around the one path:\n{}", found.join("\n"));
}

// ---------------------------------------------------------------- stamps --

fn ms(s: &Stack) -> i64 {
    s.clock.get().timestamp_millis()
}

async fn stamp_of(s: &Stack, id: &str) -> Stamp {
    s.svc.thread_stamp(id).await.expect("read").expect("the thread carries a stamp")
}

async fn row(s: &Stack, id: &str) -> Thread {
    s.edb
        .list_threads_including_deleted()
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.id_string().as_deref() == Some(id))
        .expect("the thread record")
}

fn listed(threads: &[Thread], id: &str) -> bool {
    threads.iter().any(|t| t.id_string().as_deref() == Some(id) && t.deleted_at.is_none())
}

/// S1: create, rename, re-describe and delete each set a new stamp, from this
/// device and its clock; what they write is what the store holds.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s1_create_rename_and_delete_each_carry_a_new_stamp_from_this_device() {
    let s = Stack::new("ts1-each").await;
    let id = s.svc.thread_create("Garden", "beds and seeds").await.expect("created");
    let created = stamp_of(&s, &id).await;
    assert_eq!(created.device, DEVICE, "stamped by this device");
    assert_eq!(created.wall_ms, ms(&s), "with this device's clock");
    let t = row(&s, &id).await;
    assert_eq!((t.name.as_str(), t.description.as_str()), ("Garden", "beds and seeds"));
    assert!(listed(&s.edb.list_threads().await.unwrap(), &id));

    s.clock.advance(Duration::minutes(1));
    s.svc.thread_update(&id, Some("Vegetable garden"), None).await.expect("renamed");
    let renamed = stamp_of(&s, &id).await;
    assert!(renamed > created, "a rename gets a new, later stamp");
    assert_eq!((renamed.wall_ms, renamed.device.as_str()), (ms(&s), DEVICE));
    let t = row(&s, &id).await;
    assert_eq!((t.name.as_str(), t.description.as_str()), ("Vegetable garden", "beds and seeds"), "description kept");

    s.clock.advance(Duration::minutes(1));
    s.svc.thread_update(&id, None, Some("only beds")).await.expect("re-described");
    let described = stamp_of(&s, &id).await;
    assert!(described > renamed);
    assert_eq!(row(&s, &id).await.name, "Vegetable garden", "name kept");

    s.clock.advance(Duration::minutes(1));
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &id).await.expect("deleted");
    let deletion = stamp_of(&s, &id).await;
    assert!(deletion > described, "the delete gets a new, later stamp");
    assert_eq!((deletion.wall_ms, deletion.device.as_str()), (ms(&s), DEVICE));
    assert!(row(&s, &id).await.deleted_at.is_some());
    assert!(!listed(&s.edb.list_threads().await.unwrap(), &id));
}

/// S1: "from the same device clock as the documents' (C-DOC G25)" — one
/// clock: at a single instant, document and thread writes interleaved get
/// strictly increasing stamps.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s1_threads_and_documents_are_stamped_by_one_clock() {
    let s = Stack::new("ts1-one-clock").await;
    let doc = s.create_text("Note", "first").await;
    let d1 = current_stamp(&s, &doc).await;
    let id = s.svc.thread_create("Lane", "").await.unwrap();
    let t1 = stamp_of(&s, &id).await;
    s.svc.thread_update(&id, Some("Lane two"), None).await.unwrap();
    let t2 = stamp_of(&s, &id).await;
    s.editor_save(&doc, 1, v2(vec![text_block(1, "second")])).await.unwrap();
    let d2 = current_stamp(&s, &doc).await;
    assert!(d1 < t1 && t1 < t2 && t2 < d2, "one clock, never repeating: {d1:?} {t1:?} {t2:?} {d2:?}");
}

/// S1 with G25: a thread write is never stamped lower than a stamp this device
/// has issued, even with the clock set back.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s1_a_thread_write_with_the_clock_set_back_is_still_stamped_later() {
    let s = Stack::new("ts1-set-back").await;
    s.clock.advance(Duration::minutes(10));
    let doc = s.create_text("Note", "x").await;
    let d = current_stamp(&s, &doc).await;
    s.clock.advance(Duration::minutes(-60));
    let id = s.svc.thread_create("After the doc", "").await.unwrap();
    assert!(stamp_of(&s, &id).await > d);
}

/// S1 with G25 ("the clock is initialised from the stamps the store holds"):
/// after a restart, with the clock set back, the next write — document or
/// thread — is stamped after the last thread stamp written before it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s1_after_a_restart_the_clock_starts_after_the_threads_stamps() {
    let mut s = Stack::new("ts1-restart").await;
    let _doc = s.create_text("Earlier", "x").await;
    s.clock.advance(Duration::hours(1));
    let id = s.svc.thread_create("Last write", "").await.unwrap();
    let last = stamp_of(&s, &id).await;
    s.restart();
    s.clock.advance(Duration::minutes(-59));
    let doc = s.create_text("After the restart", "y").await;
    assert!(current_stamp(&s, &doc).await > last, "a document after the restart");
    s.svc.thread_update(&id, Some("Renamed after the restart"), None).await.unwrap();
    assert!(stamp_of(&s, &id).await > last, "a thread write after the restart");
}

/// S1, "deleting … through one path, including `delete_thread_to_trash`": the
/// thread's deletion is stamped after the documents it sent to the trash.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s1_delete_thread_to_trash_stamps_the_deletion_after_its_documents() {
    let s = Stack::new("ts1-delete").await;
    let id = s.svc.thread_create("Old drafts", "").await.unwrap();
    let mut docs = vec![];
    for t in ["one", "two"] {
        let mut d = new_text_doc(t, t);
        d.thread_id = id.clone();
        docs.push(s.svc.create(d).await.unwrap().id);
    }
    let before = stamp_of(&s, &id).await;
    let report = delete_thread_to_trash(&s.svc, s.edb.as_ref(), &id).await.expect("deleted");
    assert_eq!(report.trashed.len(), 2);
    let deletion = stamp_of(&s, &id).await;
    assert!(deletion > before);
    for d in &docs {
        assert!(deletion > current_stamp(&s, d).await, "{d}: trashed before the thread was deleted");
    }
}

/// S1, "any other path": the orphan resolution's new thread (S5-12 W4) is
/// created through the path, so it is stamped.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s1_the_new_thread_of_an_orphan_resolution_is_stamped() {
    let s = Stack::new("ts1-orphan").await;
    let gone = s.svc.thread_create("Gone", "").await.unwrap();
    delete_thread_to_trash(&s.svc, s.edb.as_ref(), &gone).await.unwrap();
    s.clock.advance(Duration::minutes(1));
    let mut r: RemoteDocument = remote_text("document:arrived1", "Arrived", "text", "phone");
    r.thread_id = gone.clone();
    s.svc.apply_wire(r, meta(Some(stamp(&s, 1, 0, "phone")), None)).await.unwrap();
    s.clock.advance(Duration::minutes(1));
    let done = s
        .svc
        .resolve_orphan("document:arrived1", OrphanResolution::NewThread("Brought back".into()))
        .await
        .expect("resolved");
    let new_id = done.thread_id.expect("a new thread");
    let st = stamp_of(&s, &new_id).await;
    assert_eq!((st.wall_ms, st.device.as_str()), (ms(&s), DEVICE));
}

/// S5's starting point, at the service: a thread record written before stamps
/// (an older build, here a store write) has no stamp — none is made up.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn s1_a_thread_written_before_stamps_has_none() {
    let s = Stack::new("ts1-legacy").await;
    let id = s.edb.create_thread(Thread::new("Legacy".into(), String::new())).await.unwrap().id_string().unwrap();
    assert_eq!(s.svc.thread_stamp(&id).await.expect("read"), None);
}
