//! A device's store for the end-to-end node tests: an encrypted in-memory
//! store and the document service over it. Since step 4 of the C-DOC
//! adoption (C-WIRE) documents sync only through the document service, so a
//! node that should exchange documents needs one.
#![allow(dead_code)]

use std::sync::Arc;

use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::GraphDB;
use sovereign_docs::DocumentService;

pub async fn encrypted_store(tag: &str) -> (Arc<EncryptedGraphDB>, Arc<DocumentService>) {
    let raw = Arc::new(SurrealGraphDB::new(StorageMode::Memory).await.unwrap());
    raw.connect().await.unwrap();
    raw.init_schema().await.unwrap();
    // A counter keeps names unique when two stacks are built in the same clock tick:
    // stacks sharing a name share key-database files and their `.tmp` writes race.
    let u = format!(
        "{}-{}-{}-{tag}",
        std::process::id(),
        { static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0); SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst) },
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    );
    let kdb = |s: &str| {
        Arc::new(tokio::sync::RwLock::new(sovereign_crypto::key_db::KeyDatabase::new(
            std::env::temp_dir().join(format!("sovereign-e2e-{u}-{s}.db")),
        )))
    };
    let mk = sovereign_crypto::master_key::MasterKey::generate();
    let edb = Arc::new(EncryptedGraphDB::new(
        raw as Arc<dyn GraphDB>,
        kdb("doc"),
        kdb("msg"),
        kdb("thr"),
        kdb("conv"),
        kdb("con"),
        kdb("shr"),
        Arc::new(sovereign_crypto::kek::Kek::generate()),
        Arc::new(sovereign_crypto::index_key::IndexKey::generate()),
        Arc::new(sovereign_crypto::device_key::DeviceKey::derive(&mk, tag).unwrap()),
    ));
    let docs = Arc::new(service(edb.clone(), tag));
    (edb, docs)
}

/// The document service over `edb` with neutral host parts.
pub fn service(edb: Arc<EncryptedGraphDB>, device: &str) -> DocumentService {
    struct Clock;
    impl sovereign_docs::Clock for Clock {
        fn now(&self) -> chrono::DateTime<chrono::Utc> {
            chrono::Utc::now()
        }
    }
    struct NoEvents;
    impl sovereign_docs::DocumentEvents for NoEvents {
        fn emit(&self, _e: sovereign_docs::DocumentChanged) {}
    }
    struct NoMedia;
    impl sovereign_docs::MediaFiles for NoMedia {
        fn sha256(&self, _p: &str) -> Option<String> {
            None
        }
    }
    struct PassThrough;
    #[async_trait::async_trait]
    impl sovereign_docs::PiiIngest for PassThrough {
        async fn ingest(
            &self,
            r: sovereign_docs::IngestRequest,
        ) -> Result<Vec<sovereign_docs::IngestedItem>, sovereign_docs::PiiError> {
            Ok(r.items.into_iter().map(|i| sovereign_docs::IngestedItem { field: i.field, text: i.text }).collect())
        }
    }
    DocumentService::new(sovereign_docs::ServiceDeps {
        store: edb,
        commit_policy: Arc::new(sovereign_docs::DefaultCommitPolicy::default()),
        pii: Arc::new(PassThrough),
        media: Arc::new(NoMedia),
        clock: Arc::new(Clock),
        events: Arc::new(NoEvents),
        device: device.into(),
    })
}
