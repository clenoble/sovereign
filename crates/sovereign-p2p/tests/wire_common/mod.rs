//! Harness for the C-WIRE suite (spec ch. 05, *Sync wire — contract
//! (C-WIRE)*): two (or three) devices, each with its own encrypted in-memory
//! store, document service, sync service, identity key and clock, paired with
//! a shared pair key. A round runs what `node.rs` runs between two connected
//! nodes — manifests, the document plan, envelopes both ways, purge markers —
//! **without the network**: the envelopes are the real sealed, signed ones,
//! handed from one sync service to the other. The node's own chaining is not
//! exercised here (see TESTS.md).
#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::RwLock;

use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::IndexKey;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
pub use sovereign_db::GraphDB;
use sovereign_docs::{
    Clock, CommitContext, CommitDecision, CommitPolicy, DocumentChanged, DocumentEvents, DocumentService,
    IngestRequest, IngestedItem, MediaFiles, PiiError, PiiIngest, ServiceDeps,
};
use sovereign_p2p::protocol::manifest::SyncManifest;
use sovereign_p2p::wire::{plan_documents, DocumentPlan, IncomingAnalysis};
use sovereign_p2p::{libp2p, SyncService, VersionStore};

pub const SECRET: &str = "alice@example.com";

// ------------------------------------------------------------------ fakes --

pub struct FakeClock(Mutex<DateTime<Utc>>);

impl FakeClock {
    pub fn at(t: DateTime<Utc>) -> Self {
        Self(Mutex::new(t))
    }
    pub fn advance(&self, d: Duration) {
        *self.0.lock().unwrap() += d;
    }
    pub fn get(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.get()
    }
}

#[derive(Default)]
pub struct Events(Mutex<Vec<DocumentChanged>>);

impl DocumentEvents for Events {
    fn emit(&self, e: DocumentChanged) {
        self.0.lock().unwrap().push(e);
    }
}

pub struct NeverPolicy;

impl CommitPolicy for NeverPolicy {
    fn decide(&self, _ctx: &CommitContext) -> CommitDecision {
        CommitDecision::NotYet
    }
}

pub struct NoMedia;

impl MediaFiles for NoMedia {
    fn sha256(&self, _path: &str) -> Option<String> {
        None
    }
}

/// Pass-through, or replaces [`SECRET`] with a token-form text (the receiver
/// tokenizes what the sender left raw).
pub struct Pii {
    pub tokenize: Mutex<bool>,
    n: AtomicU32,
}

#[async_trait]
impl PiiIngest for Pii {
    async fn ingest(&self, request: IngestRequest) -> Result<Vec<IngestedItem>, PiiError> {
        let tok = *self.tokenize.lock().unwrap();
        Ok(request
            .items
            .iter()
            .map(|i| {
                let mut text = i.text.clone();
                if tok {
                    while text.contains(SECRET) {
                        let n = self.n.fetch_add(1, Ordering::SeqCst);
                        text = text.replacen(SECRET, &format!("[pii:pii_record:t{n}]"), 1);
                    }
                }
                IngestedItem { field: i.field, text }
            })
            .collect())
    }
}

/// An analysis that finds what contains "IGNORE ALL PREVIOUS", and counts
/// its calls.
#[derive(Default)]
pub struct Analysis {
    pub calls: AtomicU32,
}

#[async_trait]
impl IncomingAnalysis for Analysis {
    async fn analyse(&self, _prior: &str, incoming: &str) -> Option<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        incoming.contains("IGNORE ALL PREVIOUS").then(|| "injected instructions".to_string())
    }
}

// ----------------------------------------------------------------- device --

pub struct Device {
    pub name: String,
    pub raw: Arc<SurrealGraphDB>,
    pub edb: Arc<EncryptedGraphDB>,
    pub docs: Arc<DocumentService>,
    pub sync: Arc<SyncService>,
    pub peer: libp2p::PeerId,
    pub clock: Arc<FakeClock>,
    pub pii: Arc<Pii>,
    pub analysis: Arc<Analysis>,
}

/// A file-name stem no other device in any run can share: the process-wide
/// counter covers parallel tests reading the same instant, the pid covers
/// binaries running at once, the time covers a recycled pid on a later run.
fn uniq(tag: &str) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    format!("{}-{n}-{nanos}-{tag}", std::process::id())
}

/// The real time, rounded: devices start in step unless a test moves a clock.
pub fn now() -> DateTime<Utc> {
    Utc::now()
}

pub const ACCOUNT_SEED: [u8; 32] = [7u8; 32];

pub async fn device(name: &str, seed: u8) -> Device {
    let raw = Arc::new(SurrealGraphDB::new(StorageMode::Memory).await.unwrap());
    raw.connect().await.unwrap();
    raw.init_schema().await.unwrap();
    let u = uniq(name);
    let dir = std::env::temp_dir();
    let kdb = |s: &str| Arc::new(RwLock::new(KeyDatabase::new(dir.join(format!("sovereign-wire-{u}-{s}.db")))));
    let device_key = DeviceKey::derive(&MasterKey::generate(), name).unwrap();
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
    let clock = Arc::new(FakeClock::at(now()));
    let pii = Arc::new(Pii { tokenize: Mutex::new(false), n: AtomicU32::new(0) });
    let analysis = Arc::new(Analysis::default());
    let kp = libp2p::identity::Keypair::ed25519_from_bytes([seed; 32]).unwrap();
    let peer = kp.public().to_peer_id();
    let docs = Arc::new(DocumentService::new(ServiceDeps {
        store: edb.clone(),
        commit_policy: Arc::new(NeverPolicy),
        pii: pii.clone(),
        media: Arc::new(NoMedia),
        clock: clock.clone(),
        events: Arc::new(Events::default()),
        device: peer.to_string(),
    }));
    let transport = AccountKey::from_bytes(ACCOUNT_SEED).derive_transport_key();
    let sync = SyncService::new(
        edb.clone() as Arc<dyn GraphDB>,
        peer.to_string(),
        transport,
        kp,
        VersionStore::ephemeral(),
    )
    .with_documents(docs.clone())
    .with_analysis(analysis.clone());
    Device { name: name.into(), raw, edb, docs, sync: Arc::new(sync), peer, clock, pii, analysis }
}

/// Pairs two devices: the shared pair key on both sides.
pub fn pair(a: &Device, b: &Device) {
    let key = AccountKey::from_bytes(ACCOUNT_SEED).derive_pair_key(&a.peer.to_string(), &b.peer.to_string());
    a.sync.add_pair_key(b.peer.to_string(), key);
    b.sync.add_pair_key(a.peer.to_string(), key);
}

pub async fn manifest(d: &Device) -> SyncManifest {
    d.sync.build_manifest().await.expect("manifest")
}

/// One sync round between `a` (initiator) and `b`, as `node.rs` runs it.
pub async fn round(a: &Device, b: &Device) -> DocumentPlan {
    let (ma, mb) = (manifest(a).await, manifest(b).await);
    let plan = plan_documents(&ma, &mb);
    for doc in plan.fetch.iter().chain(plan.concurrent.iter()) {
        let env = b.sync.get_commits_since(doc, None, &a.peer).await.expect("seal from b");
        a.sync.apply_commits(env, &b.peer).await.expect("apply on a");
    }
    for doc in plan.push.iter().chain(plan.concurrent.iter()) {
        let env = a.sync.get_commits_since(doc, None, &b.peer).await.expect("seal from a");
        b.sync.apply_commits(env, &a.peer).await.expect("apply on b");
    }
    a.sync.apply_purge_markers(&mb).await.expect("markers on a");
    b.sync.apply_purge_markers(&ma).await.expect("markers on b");
    plan
}

/// Rounds until a round has no document work (at most `max`); returns how
/// many rounds had work.
pub async fn settle(a: &Device, b: &Device, max: usize) -> usize {
    for n in 0..max {
        let p = round(a, b).await;
        if p.fetch.is_empty() && p.push.is_empty() && p.concurrent.is_empty() {
            return n;
        }
    }
    max
}

/// A manifest as a peer on an older build would send it.
pub fn as_old_build(mut m: SyncManifest) -> SyncManifest {
    m.wire_version = None;
    m.document_wire.clear();
    m.purge_markers.clear();
    m
}

pub fn texts(stored: &str) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_str(stored).expect("v2 JSON");
    v["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["type"] == "text")
        .map(|b| b["text"].as_str().unwrap_or_default().to_string())
        .collect()
}

pub fn v2_text(text: &str) -> String {
    serde_json::json!({"format": "sovereign.doc", "v": 2,
        "blocks": [{"type": "text", "id": "00000000000000000000000000001001", "markup": "markdown", "text": text}]})
    .to_string()
}

/// Saves `text` from the editor of device `d`.
pub async fn edit(d: &Device, id: &str, text: &str) {
    let base = d.docs.read(id).await.unwrap().revision;
    d.docs
        .save(id, sovereign_docs::Edit {
            base,
            title: None,
            content: Some(sovereign_docs::EditContent::V2(v2_text(text))),
            origin: sovereign_docs::Origin::Editor { session: sovereign_docs::EditSession(1) },
            explicit: false,
            pasted_tokens: vec![],
        })
        .await
        .unwrap();
}

pub async fn create(d: &Device, title: &str, text: &str) -> String {
    let new = sovereign_docs::NewDocument {
        title: title.into(),
        thread_id: "thread:t1".into(),
        content: sovereign_docs::NewContent::Text(text.into()),
        trust: sovereign_docs::Trust::Owned,
        provenance: sovereign_docs::Provenance::Written,
        origin: sovereign_docs::Origin::Editor { session: sovereign_docs::EditSession(1) },
        created_at: None,
        modified_at: None,
    };
    d.docs.create(new).await.unwrap().id
}

pub async fn stored_text(d: &Device, id: &str) -> Vec<String> {
    texts(&d.edb.get_document(id).await.expect("stored").content)
}

/// (title, content) of every version of `id` on `d`.
pub async fn history(d: &Device, id: &str) -> Vec<Vec<String>> {
    d.edb.list_document_commits(id).await.unwrap_or_default().into_iter().map(|c| texts(&c.snapshot.content)).collect()
}
