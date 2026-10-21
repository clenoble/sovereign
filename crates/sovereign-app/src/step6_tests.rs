//! C-DOC adoption step 6, **M5 — the decoy is seeded in the new format**
//! (`doc/plans/C-DOC-adoption.md`, *Step 6*, "Settled after the test
//! author's report", 2026-10-18; PANDA2's review points, claude-coord 0112).
//! Written before the code, by another author: whoever implements step 6 makes
//! these pass without editing this file.
//!
//! A decoy seeded after the update holds no legacy rows, so on a fresh install
//! neither persona gets a migration or a notice. Checked through the shared
//! seed function (`duress::seed_duress_db`) plus a scan that its four callers
//! (onboarding pre-seed and first duress login in `tauri_commands/auth.rs`,
//! `tauri_commands/recovery.rs`, `tauri_commands/access_recovery.rs`) all go
//! through it. The rows' dates stay spread over the past (`past_ts`), not one
//! instant.
//!
//! Out of scope, unchanged: the decoy's raw store, its file name, the shell
//! (which seeds no decoy). Installs that already hold a legacy decoy are
//! covered in `sovereign-docs/tests/step6_unlock_migration.rs` (M5, equal
//! treatment).

use std::collections::HashSet;
use std::sync::{mpsc, Arc, Mutex};

use serde_json::Value;
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::IndexKey;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::GraphDB;
use sovereign_docs::{run_unlock_migration, start_unlock_migration, MigrationRun, MigrationTask};

async fn seeded_decoy() -> Arc<SurrealGraphDB> {
    let raw = Arc::new(SurrealGraphDB::new(StorageMode::Memory).await.unwrap());
    raw.connect().await.unwrap();
    raw.init_schema().await.unwrap();
    crate::duress::seed_duress_db(&raw, "desktop").await.expect("seed the decoy");
    raw
}

/// The decoy's store through the encrypted layer, with its own keys (as the
/// duress login wraps it), and the document service over it.
fn service_over(raw: Arc<SurrealGraphDB>, keys: &std::path::Path) -> Arc<sovereign_docs::DocumentService> {
    let kdb = |s: &str| Arc::new(tokio::sync::RwLock::new(KeyDatabase::new(keys.join(format!("k-{s}.db")))));
    let mk = MasterKey::generate();
    let edb = Arc::new(EncryptedGraphDB::new(
        raw as Arc<dyn GraphDB>,
        kdb("doc"),
        kdb("msg"),
        kdb("thr"),
        kdb("conv"),
        kdb("con"),
        kdb("shr"),
        Arc::new(Kek::generate()),
        Arc::new(IndexKey::generate()),
        Arc::new(DeviceKey::derive(&mk, "this-device").unwrap()),
    ));
    Arc::new(sovereign_ai::docs_host::build_document_service(edb, None, "desktop".into()))
}

fn v2_texts(stored: &str) -> Option<Vec<String>> {
    let v: Value = serde_json::from_str(stored).ok()?;
    if v["format"] != "sovereign.doc" || v["v"] != 2 {
        return None;
    }
    Some(
        v["blocks"]
            .as_array()?
            .iter()
            .filter(|b| b["type"] == "text")
            .map(|b| b["text"].as_str().unwrap_or_default().to_string())
            .collect(),
    )
}

/// Every document the seed writes is in format v2 (checked with `serde_json`,
/// not the format crate), with its text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m5_a_decoy_seeded_now_holds_no_legacy_rows() {
    let raw = seeded_decoy().await;
    let docs = raw.list_documents_including_deleted().await.unwrap();
    assert!(!docs.is_empty(), "the decoy is seeded with documents");
    for d in &docs {
        let texts = v2_texts(&d.content)
            .unwrap_or_else(|| panic!("a decoy document is not in format v2 ({} bytes)", d.content.len()));
        assert!(texts.iter().any(|t| !t.trim().is_empty()), "its text is kept");
    }
}

/// A freshly seeded decoy gives the migration nothing to convert, so its first
/// unlock shows no notice — as a real persona created after the update.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m5_a_freshly_seeded_decoy_gives_the_migration_nothing_and_no_notice() {
    let keys = tempfile::tempdir().unwrap();
    let svc = service_over(seeded_decoy().await, keys.path());

    let report = svc.migrate_all().await.expect("migrate_all");
    assert!(report.migrated.is_empty(), "nothing to convert: {} converted", report.migrated.len());
    assert!(report.read_only.is_empty(), "nothing unreadable");
    assert!(report.failed.is_empty());

    let slot: Arc<Mutex<Option<MigrationTask>>> = Arc::new(Mutex::new(None));
    let keep = slot.clone();
    let (tx, rx) = mpsc::channel();
    start_unlock_migration(
        svc.clone(),
        move |t| *keep.lock().unwrap() = Some(t),
        move |n| {
            let _ = tx.send(n);
        },
    );
    let task = slot.lock().unwrap().take().expect("task");
    tokio::time::timeout(std::time::Duration::from_secs(60), task).await.expect("bounded");
    assert!(rx.try_recv().is_err(), "no notice at the decoy's first unlock");
    assert_eq!(run_unlock_migration(&svc).await.unwrap(), MigrationRun::AlreadyComplete);
}

/// The seeded rows' dates stay spread over the past (`past_ts`): not one
/// instant, none in the future, within the seed's ~6 months.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m5_the_seeded_decoy_rows_keep_spread_dates() {
    let now = chrono::Utc::now();
    let raw = seeded_decoy().await;
    let docs = raw.list_documents_including_deleted().await.unwrap();
    let created: HashSet<_> = docs.iter().map(|d| d.created_at).collect();
    assert!(
        created.len() * 2 >= docs.len() && created.len() > 1,
        "dates spread, not one instant: {} distinct over {} documents",
        created.len(),
        docs.len()
    );
    for d in &docs {
        assert!(d.created_at <= now, "no date in the future");
        assert!(now - d.created_at <= chrono::Duration::days(181), "within the seed's span");
        assert!(d.modified_at >= d.created_at, "modified no earlier than created");
    }
}

/// Every place that seeds a decoy goes through the one seed function: the four
/// callers PANDA2 listed (onboarding pre-seed and first duress login in
/// `auth.rs`, `recovery.rs`, `access_recovery.rs`) call it, and no other
/// production file does. (No other writer of decoy documents exists: the
/// writers gate in `sovereign-docformat/tests/readers_gate.rs` allows
/// `.create_document(` in `duress.rs` only.)
#[test]
fn m5_every_decoy_seeding_goes_through_the_one_seed_function() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut calls: Vec<(String, usize)> = vec![];
    let mut stack = vec![src.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let rel = p.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
            if !rel.ends_with(".rs") || rel.ends_with("_tests.rs") || rel == "duress.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            let n = text
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .filter(|l| l.contains("seed_duress_db("))
                .count();
            if n > 0 {
                calls.push((rel, n));
            }
        }
    }
    calls.sort();
    assert_eq!(
        calls,
        vec![
            ("tauri_commands/access_recovery.rs".to_string(), 1),
            ("tauri_commands/auth.rs".to_string(), 2),
            ("tauri_commands/recovery.rs".to_string(), 1),
        ],
        "the decoy is seeded through `duress::seed_duress_db` by its four callers, and nowhere else"
    );
}
