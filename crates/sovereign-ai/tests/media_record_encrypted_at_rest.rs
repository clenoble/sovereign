//! The record of what C-MEDIA's migration pass neutralises is encrypted at rest.
//!
//! Spec ch. 03, *Received content is hostile — contract (C-MEDIA)*, *The media
//! store*, v2.0.85: "The record lives in the persona's area, encrypted, outside
//! every synced store [...] Its encryption has its test. A former path holding a
//! marker string is neutralised, and afterwards no file under the profile holds
//! that string in clear."
//!
//! A real store on disk (SurrealKV) under a fresh test profile in the temp
//! folder, the key files beside it, and the document service as the hosts build
//! it with its media area under the same profile
//! (`build_document_service_in_area`). Nothing touches `~/.sovereign`: no
//! `SOVEREIGN_DATA_DIR` is read by this path. The marker is looked for in every
//! file under the profile, after the store is closed; then the store is reopened
//! under the same profile and keys, and the record must still hold the entry.
//! The two halves hold each other: the record is on disk, and not in clear.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use sovereign_ai::docs_host::{build_document_service_in_area, testing};
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::IndexKey;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Document;
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::traits::GraphDB;
use sovereign_docs::NeutralisedReason;

/// A string no real file holds by chance.
const MARKER: &str = "MARKER-l1b-91c2e7-record";

struct Profile(PathBuf);
impl Drop for Profile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => files_under(&p, out),
            Ok(t) if t.is_file() => out.push(p),
            _ => {}
        }
    }
}

/// The persona's keys, kept in memory across a close and a reopen (what an
/// unlock derives again from her password).
struct Keys {
    master: MasterKey,
    kek: Arc<Kek>,
    index: Arc<IndexKey>,
}

impl Keys {
    fn new() -> Keys {
        Keys { master: MasterKey::generate(), kek: Arc::new(Kek::generate()), index: Arc::new(IndexKey::generate()) }
    }
}

/// Opens (or reopens) the store under `profile`: the database on disk and
/// the key databases beside it, loaded when they exist.
async fn store_on_disk(profile: &Path, keys: &Keys) -> Arc<EncryptedGraphDB> {
    let db_dir = profile.join("db");
    let raw = SurrealGraphDB::new(StorageMode::Persistent(db_dir.to_string_lossy().to_string())).await.expect("store on disk");
    raw.connect().await.expect("connect");
    raw.init_schema().await.expect("schema");
    let dir = profile.join("keys");
    std::fs::create_dir_all(&dir).unwrap();
    let kdb = |s: &str| {
        let path = dir.join(format!("{s}.db"));
        let db = if path.exists() {
            KeyDatabase::load(&path, &keys.kek).expect("the key database reopens under the same key")
        } else {
            KeyDatabase::new(path)
        };
        Arc::new(tokio::sync::RwLock::new(db))
    };
    let device_key = DeviceKey::derive(&keys.master, "test-device").expect("device key");
    Arc::new(EncryptedGraphDB::new(
        Arc::new(raw) as Arc<dyn GraphDB>,
        kdb("doc"),
        kdb("msg"),
        kdb("thr"),
        kdb("conv"),
        kdb("con"),
        kdb("shr"),
        keys.kek.clone(),
        keys.index.clone(),
        Arc::new(device_key),
    ))
}

// The media store, the record (v2.0.85): a neutralised former path holding a marker is in the record; no file under the profile holds the marker in clear; and the record is on disk: reopened under the same profile and keys, it still holds the entry (claude-coord 0032: without the reopen, a record kept in memory would pass).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn media_record_holds_no_path_in_clear_under_the_profile() {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let profile = Profile(std::env::temp_dir().join(format!("sov-l1b-profile-{}-{nanos}", std::process::id())));
    std::fs::create_dir_all(&profile.0).unwrap();
    let former_path = format!("C:/Users/someone/{MARKER}/holiday.png");
    let keys = Keys::new();
    {
        let store = store_on_disk(&profile.0, &keys).await;
        let docs = build_document_service_in_area(
            store.clone(),
            Some(testing::account_key()),
            "desktop".into(),
            &profile.0.join("media"),
        );
        // A document from before the update, written here, naming a file that is not there.
        let content = json!({"format": "sovereign.doc", "v": 2, "blocks": [
            {"type": "text", "id": format!("{:032x}", 1), "markup": "markdown", "text": "x"},
            {"type": "image", "id": format!("{:032x}", 2), "ref": {"path": former_path, "sha256": "ab".repeat(32)}, "alt": "a picture", "caption": ""}
        ]});
        let mut d = Document::new("Before the update".into(), "thread:t1".into(), true);
        d.content = content.to_string();
        store.create_document(d).await.expect("a row from before the update");

        let report = docs.migrate_media_into_store().await.expect("the pass");
        assert_eq!(report.neutralised, 1, "precondition: the block is neutralised");
        let record = docs.neutralised_media().await.expect("the record");
        assert!(record.iter().any(|e| e.former_path == former_path), "precondition: the record holds the former path");
    }
    // The service and the store are dropped: whatever they keep is on disk.
    let mut files = vec![];
    files_under(&profile.0, &mut files);
    assert!(!files.is_empty(), "precondition: the profile holds the store's files");
    for f in &files {
        let bytes = std::fs::read(f).unwrap_or_default();
        assert!(
            !bytes.windows(MARKER.len()).any(|w| w == MARKER.as_bytes()),
            "{} holds the recorded path in clear",
            f.strip_prefix(&profile.0).unwrap_or(f).display()
        );
    }

    // The other half: the record is on disk. Reopened under the same profile,
    // with the same keys, it still holds the entry.
    let store = store_on_disk(&profile.0, &keys).await;
    let docs = build_document_service_in_area(store, Some(testing::account_key()), "desktop".into(), &profile.0.join("media"));
    let record = docs.neutralised_media().await.expect("the record, after a reopen");
    assert!(
        record.iter().any(|e| e.former_path == former_path && e.reason == NeutralisedReason::Absent),
        "after a reopen the record still holds the entry, with its former path and its reason"
    );
}
