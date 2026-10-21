//! C-DOC adoption step 6, **M5 on disk** (`doc/plans/C-DOC-adoption.md`,
//! *Step 6 … contract*): "no difference observable in … the on-disk traces it
//! leaves (the per-store record is as neutral as the store itself)". Written
//! before the code, by another author: whoever implements step 6 makes this
//! pass without editing this file.
//!
//! Two persona stores on one device, as a host has them: each a persistent
//! store (RocksDB on desktop, SurrealKV on mobile) with its key databases, in
//! its own directory under a **neutral** name chosen here (`slots/a`,
//! `slots/b`), so that any persona word that appears on disk comes from the
//! migration and not from today's file layout (ATREST-011's file names are
//! C-SESS work, not step 6). Slot `a` is an upgraded real workspace (legacy
//! rows sealed); slot `b` is a decoy seeded the way `src/duress.rs` seeds it
//! (plaintext rows in the raw store). Both run the migration through the
//! host seam, to completion.
//!
//! What must hold: nothing is written outside the two persona directories
//! (the record lives in the persona's store, not in a shared file); no file
//! the migration creates or changes is named after a persona; none holds a
//! persona word; and the two personas get the same notice.
//!
//! One test in its own binary: it sets `SOVEREIGN_DATA_DIR` to its own
//! temporary directory before anything reads it, so no other test races it
//! and nothing can reach a real workspace.

#![cfg(feature = "encryption")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};

use chrono::{TimeZone, Utc};
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::IndexKey;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Document;
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::GraphDB;
use sovereign_docs::{start_unlock_migration, unlock_migration_complete, DocumentService, MigrationNotice};

/// Words that would tell the personas apart if the migration wrote them.
const PERSONA_WORDS: [&str; 4] = ["duress", "decoy", "primary", "persona"];

/// The decoy's documents, as `seed_duress_db` writes them (a sample).
const DOCS: [(&str, &str); 3] = [
    ("Grocery List", "- Milk\n- Eggs\n- Bread"),
    ("Meeting Notes - Q1 Review", "Attendees: Team leads\n\n## Key Points"),
    ("Banana Bread", "## Ingredients\n- 3 ripe bananas"),
];

struct Slot {
    raw: Arc<SurrealGraphDB>,
    edb: Arc<EncryptedGraphDB>,
    docs: Arc<DocumentService>,
}

async fn open_slot(dir: &Path) -> Slot {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("store").to_string_lossy().replace('\\', "/");
    let raw = Arc::new(SurrealGraphDB::new(StorageMode::Persistent(path)).await.expect("persistent store"));
    raw.connect().await.unwrap();
    raw.init_schema().await.unwrap();
    let kdb = |s: &str| Arc::new(tokio::sync::RwLock::new(KeyDatabase::new(dir.join(format!("k-{s}.db")))));
    let mk = MasterKey::generate();
    let device_key = DeviceKey::derive(&mk, "this-device").unwrap();
    let edb = Arc::new(EncryptedGraphDB::new(
        raw.clone() as Arc<dyn GraphDB>,
        kdb("doc"),
        kdb("msg"),
        kdb("thr"),
        kdb("conv"),
        kdb("con"),
        kdb("shr"),
        Arc::new(Kek::generate()),
        Arc::new(IndexKey::generate()),
        Arc::new(device_key),
    ));
    let docs = Arc::new(sovereign_ai::docs_host::build_document_service(edb.clone(), None, "desktop".into()));
    Slot { raw, edb, docs }
}

fn legacy_row(title: &str, body: &str) -> Document {
    let mut d = Document::new(title.to_string(), "thread:t1".to_string(), true);
    d.content = body.to_string();
    d.created_at = Utc.with_ymd_and_hms(2025, 5, 1, 9, 0, 0).unwrap();
    d.modified_at = d.created_at;
    d
}

/// Every file under `root`, with its bytes (`None` when it cannot be read
/// right now, e.g. a store's lock file held open).
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.insert(p.strip_prefix(root).unwrap().to_path_buf(), std::fs::read(&p).ok());
            }
        }
    }
    out
}

fn contains_word(bytes: &[u8], word: &str) -> bool {
    let w = word.as_bytes();
    bytes.windows(w.len()).any(|win| win.eq_ignore_ascii_case(w))
}

/// Runs the migration through the host seam, as a host's unlock does, and
/// waits for it (bounded).
async fn unlock(slot: &Slot) -> Option<MigrationNotice> {
    let handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>> = Arc::new(Mutex::new(None));
    let keep = handle.clone();
    let (tx, rx) = mpsc::channel();
    start_unlock_migration(
        slot.docs.clone(),
        move |task| *keep.lock().unwrap() = Some(tokio::spawn(task)),
        move |n| {
            let _ = tx.send(n);
        },
    );
    let h = handle.lock().unwrap().take().expect("the task is spawned");
    tokio::time::timeout(std::time::Duration::from_secs(120), h).await.expect("bounded").expect("task");
    rx.try_recv().ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m5_the_migration_leaves_no_persona_trace_on_disk() {
    let data = tempfile::tempdir().expect("temp dir");
    std::env::set_var("SOVEREIGN_DATA_DIR", data.path());
    let a_dir = data.path().join("slots").join("a");
    let b_dir = data.path().join("slots").join("b");

    let real = open_slot(&a_dir).await;
    let decoy = open_slot(&b_dir).await;
    for (title, body) in DOCS {
        // The real persona: an upgraded workspace, its legacy rows sealed
        // (through the encrypted layer, legacy in form).
        real.edb.create_document(legacy_row(title, body)).await.unwrap();
        // The decoy: plaintext, straight into the raw store (src/duress.rs).
        decoy.raw.create_document(legacy_row(title, body)).await.unwrap();
    }
    assert!(
        !snapshot(&a_dir).is_empty() && !snapshot(&b_dir).is_empty(),
        "no persistent backend compiled in: the stores wrote no files, so this test would prove nothing"
    );

    let before = snapshot(data.path());
    let n_real = unlock(&real).await;
    let n_decoy = unlock(&decoy).await;
    let after = snapshot(data.path());

    assert_eq!(n_real, Some(MigrationNotice { converted: DOCS.len(), not_converted: 0 }));
    assert_eq!(n_real, n_decoy, "the same notice for both personas");
    assert!(unlock_migration_complete(&real.docs).await.unwrap());
    assert!(unlock_migration_complete(&decoy.docs).await.unwrap());

    let slots = [PathBuf::from("slots").join("a"), PathBuf::from("slots").join("b")];
    let mut problems = vec![];
    for (path, bytes) in &after {
        let changed = match before.get(path) {
            None => true,
            Some(old) => old != bytes,
        };
        if !changed {
            continue;
        }
        if !slots.iter().any(|s| path.starts_with(s)) {
            problems.push(format!("written outside the persona directories: {}", path.display()));
        }
        let name = path.to_string_lossy().to_lowercase();
        for w in PERSONA_WORDS {
            if name.contains(w) {
                problems.push(format!("a file named after a persona ({w}): {}", path.display()));
            }
            if let Some(b) = bytes {
                if contains_word(b, w) {
                    problems.push(format!("a persona word ({w}) in {}", path.display()));
                }
            }
        }
    }
    assert!(problems.is_empty(), "on-disk traces that tell the personas apart (M5):\n{}", problems.join("\n"));
}

