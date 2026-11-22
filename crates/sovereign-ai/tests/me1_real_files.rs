//! ME1 over real files — a received path is never opened, by the shipped host.
//!
//! Spec ch. 03, *Received content is hostile — contract (C-MEDIA)*: "**ME1 — A
//! received path is never opened.** A state that arrives whole from outside the
//! device never leads to a read of a path it names. That covers sync, a
//! restored backup, and a document imported as a Sovereign document."
//!
//! The service as the hosts build it (`build_document_service`, with the
//! shipped `FileMedia`), over a real file this device holds: a sentinel in a
//! fresh temp folder. A read of it would leave its hash in the stored block,
//! so the observation needs no timing and no test double. This is the review's
//! finding 1 (v0.0.12, `review-2026-11-04-cloud`), exercised end to end.
//!
//! The store is in memory and the key files in the temp folder; nothing touches
//! `~/.sovereign` (no `SOVEREIGN_DATA_DIR` is read by this path).

use std::path::PathBuf;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sovereign_ai::docs_host::{build_document_service, testing};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{BackupDocument, RemoteDocument, Stamp, Trust, WireMeta};

struct Sentinel {
    dir: PathBuf,
    path: String,
    sha: String,
}

impl Sentinel {
    fn new(tag: &str) -> Sentinel {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("sov-me1-{}-{nanos}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("sentinel.png");
        let bytes = format!("sentinel {tag} {nanos}");
        std::fs::write(&file, &bytes).unwrap();
        let sha = Sha256::digest(bytes.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        Sentinel { dir, path: file.to_string_lossy().replace('\\', "/"), sha }
    }
}

impl Drop for Sentinel {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn remote(id: &str, path: &str) -> RemoteDocument {
    let content = json!({"format": "sovereign.doc", "v": 2, "blocks": [
        {"type": "text", "id": format!("{:032x}", 1), "markup": "markdown", "text": "received"},
        {"type": "image", "id": format!("{:032x}", 2), "ref": {"path": path}, "alt": "a picture", "caption": ""}
    ]});
    RemoteDocument {
        id: id.into(),
        title: "Received".into(),
        content: content.to_string(),
        thread_id: "thread:t1".into(),
        trust: Some(Trust::Owned),
        provenance: None,
        modified_at: chrono::Utc::now(),
        deleted_at: None,
        device: "phone".into(),
        trust_decision: None,
    }
}

fn meta_now() -> WireMeta {
    WireMeta {
        stamp: Some(Stamp { wall_ms: chrono::Utc::now().timestamp_millis(), counter: 0, device: "phone".into() }),
        ..WireMeta::default()
    }
}

async fn stored_image_hash(store: &sovereign_db::encrypted::EncryptedGraphDB, id: &str) -> Value {
    let d = store.get_document(id).await.expect("the received document is stored");
    let v: Value = serde_json::from_str(&d.content).expect("stored v2");
    v["blocks"].as_array().unwrap().iter().find(|b| b["type"] == "image").expect("image block")["ref"]["sha256"].clone()
}

// ME1, sync over the shipped host: the received path names a real file here, and it is not read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me1_apply_wire_never_opens_received_path_real_file() {
    let sentinel = Sentinel::new("wire");
    let store = testing::encrypted_memory_store("me1-wire").await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), "desktop".into());
    let id = "document:me1real";
    docs.apply_wire(remote(id, &sentinel.path), meta_now()).await.expect("applied");
    assert_ne!(stored_image_hash(&store, id).await, Value::from(sentinel.sha.clone()), "the received path was read");
}

// ME1, a restored backup over the shipped host.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me1_restore_backup_never_opens_received_path_real_file() {
    let sentinel = Sentinel::new("backup");
    let store = testing::encrypted_memory_store("me1-backup").await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), "desktop".into());
    let id = "document:me1realbackup";
    docs.restore_backup(vec![BackupDocument { document: remote(id, &sentinel.path), meta: meta_now() }], vec![])
        .await
        .expect("restored");
    assert_ne!(stored_image_hash(&store, id).await, Value::from(sentinel.sha.clone()), "the backup's path was read");
}
