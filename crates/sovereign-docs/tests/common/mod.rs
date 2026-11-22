//! Shared harness for the C-DOC / C-ENC suite.
//!
//! Every test runs the service over the real stack: `EncryptedGraphDB` over a
//! `FaultyDb` (crash injection, generated) over an in-memory `SurrealGraphDB`.
//! The raw store and the encrypted layer are kept so a test can inspect what is
//! really stored, independently of the service's own answers.
//!
//! Stored content is checked with `serde_json`, never with
//! `sovereign-docformat`, so the suite does not trust the model it tests.
#![allow(dead_code)]

pub mod adoption;
pub mod faulty;
pub mod scan;
pub mod wire;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::{json, Value};
use tokio::sync::RwLock;

use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::IndexKey;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{
    Commit, Document, PiiKind, PiiRecord, ReviewState, SourceKind, SourceRef,
};
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
pub use sovereign_db::traits::GraphDB;

use sovereign_docs::{
    BlockId, Clock, CommitContext, CommitDecision, CommitPolicy, DefaultCommitPolicy, DocId,
    DocumentChanged, DocumentEvents, DocumentService, Edit, EditContent, EditSession,
    IngestRequest, IngestedItem, MediaError, MediaFiles, MediaMark, NewContent, NewDocument, Origin, PiiError,
    PiiIngest, Provenance, Revision, ServiceDeps, StoredMedium, TextField, Trust,
};

/// The name of this device in every stack.
pub const DEVICE: &str = "laptop";

use faulty::FaultyDb;

// ------------------------------------------------------------------ fakes --

/// A clock the test moves by hand. Starts 2026-10-10 12:00 UTC.
pub struct FakeClock(Mutex<DateTime<Utc>>);

impl FakeClock {
    pub fn new() -> Self {
        Self(Mutex::new(Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap()))
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

/// Records every event.
#[derive(Default)]
pub struct RecordingEvents(Mutex<Vec<DocumentChanged>>);

impl RecordingEvents {
    pub fn take(&self) -> Vec<DocumentChanged> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
    pub fn peek(&self) -> Vec<DocumentChanged> {
        self.0.lock().unwrap().clone()
    }
}

impl DocumentEvents for RecordingEvents {
    fn emit(&self, event: DocumentChanged) {
        self.0.lock().unwrap().push(event);
    }
}

/// How the fake personal-data hook behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PiiMode {
    /// Returns every block unchanged.
    PassThrough,
    /// Replaces each occurrence of [`SECRET`] by a token of a record it
    /// creates, linked to the document.
    Tokenize,
    /// Fails.
    Fail,
    /// Answers with a block id it was not given.
    Rogue,
}

/// A personal-data value the fake hook recognises.
pub const SECRET: &str = "alice@example.com";

pub struct FakePii {
    pub mode: Mutex<PiiMode>,
    pub requests: Mutex<Vec<IngestRequest>>,
    pub db: Mutex<Option<Arc<EncryptedGraphDB>>>,
}

impl FakePii {
    pub fn new() -> Self {
        Self { mode: Mutex::new(PiiMode::PassThrough), requests: Mutex::new(vec![]), db: Mutex::new(None) }
    }
    pub fn set(&self, m: PiiMode) {
        *self.mode.lock().unwrap() = m;
    }
    pub fn requests(&self) -> Vec<IngestRequest> {
        self.requests.lock().unwrap().clone()
    }
    pub fn clear(&self) {
        self.requests.lock().unwrap().clear();
    }
}

#[async_trait]
impl PiiIngest for FakePii {
    async fn ingest(&self, request: IngestRequest) -> Result<Vec<IngestedItem>, PiiError> {
        self.requests.lock().unwrap().push(request.clone());
        let mode = *self.mode.lock().unwrap();
        match mode {
            PiiMode::PassThrough => Ok(request
                .items
                .iter()
                .map(|i| IngestedItem { field: i.field, text: i.text.clone() })
                .collect()),
            PiiMode::Fail => Err(PiiError { message: "ingest unavailable".into() }),
            PiiMode::Rogue => Ok(vec![IngestedItem {
                field: TextField::Text(BlockId([0xEE; 16])),
                text: "rogue".into(),
            }]),
            PiiMode::Tokenize => {
                let db = self.db.lock().unwrap().clone().expect("db wired");
                let mut out = vec![];
                for i in &request.items {
                    let mut text = i.text.clone();
                    while let Some(pos) = text.find(SECRET) {
                        let rec = new_record(&db, &[&request.document_id]).await;
                        let token = format!("[pii:{rec}]");
                        text.replace_range(pos..pos + SECRET.len(), &token);
                    }
                    out.push(IngestedItem { field: i.field, text });
                }
                Ok(out)
            }
        }
    }
}

impl FakePii {
    /// The fields of the last request for `doc`.
    pub fn fields_of_last(&self, doc: &str) -> Vec<TextField> {
        self.requests()
            .into_iter()
            .rev()
            .find(|r| r.document_id == doc)
            .map(|r| r.items.into_iter().map(|i| i.field).collect())
            .unwrap_or_default()
    }
}

/// A commit policy the test drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PolicyMode {
    Never,
    Always,
    /// Delegates to the shipped `DefaultCommitPolicy` with this threshold.
    Threshold(u64),
}

pub struct FakePolicy {
    pub mode: Mutex<PolicyMode>,
    pub asked: Mutex<Vec<CommitContext>>,
}

impl FakePolicy {
    pub fn new() -> Self {
        Self { mode: Mutex::new(PolicyMode::Never), asked: Mutex::new(vec![]) }
    }
    pub fn set(&self, m: PolicyMode) {
        *self.mode.lock().unwrap() = m;
    }
}

impl CommitPolicy for FakePolicy {
    fn decide(&self, ctx: &CommitContext) -> CommitDecision {
        self.asked.lock().unwrap().push(ctx.clone());
        match *self.mode.lock().unwrap() {
            PolicyMode::Never => CommitDecision::NotYet,
            PolicyMode::Always => CommitDecision::Version,
            PolicyMode::Threshold(t) => DefaultCommitPolicy::new(t).decide(ctx),
        }
    }
}

/// Media files the test pretends this device holds: path → sha256 hex, and
/// the file's first bytes. Whether a file is a medium is decided by the
/// contract's own `is_medium` on those bytes, never by the fake. It records every path
/// the service hands it (C-MEDIA: a received or hostile path must never reach
/// it), how many first bytes the migration pass asked for, and copies into a
/// pretend media store, with the mark the service asked for, which it can be
/// asked about by hash.
#[derive(Default)]
pub struct FakeMedia {
    /// path → (sha256, the file's first bytes).
    files: Mutex<HashMap<String, (String, Vec<u8>)>>,
    /// Every path `sha256` was asked for, in order.
    hashed: Mutex<Vec<String>>,
    /// Every path `copy_into_store` (insertion) was asked for, in order.
    copied: Mutex<Vec<String>>,
    /// Every (path, max) `first_bytes` (the pass) was asked for, in order.
    sniffed: Mutex<Vec<(String, usize)>>,
    /// Every path `copy_into_store_checked` (the pass's full read) was asked
    /// for, in order.
    verified: Mutex<Vec<String>>,
    /// The pretend store: (name, sha256, mark) of every medium copied in.
    stored: Mutex<Vec<(String, String, Option<MediaMark>)>>,
    /// Files above the cap (their first bytes read fine).
    too_large: Mutex<Vec<String>>,
    /// Files that are there but cannot be read.
    unreadable: Mutex<Vec<String>>,
}

/// The store's name the fake gives the medium of hash `sha` (C-MEDIA tests).
pub fn store_name(sha: &str) -> String {
    format!("media-store-{sha}")
}

/// First bytes the fake serves: a PNG header for a medium, text for a file
/// that is not one (a text file renamed `.png`).
const PNG_HEAD: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x10\x00\x00\x00\x10\x08\x06\x00\x00\x00";
const TEXT_HEAD: &[u8] = b"These are my notes, not a picture. They go on for a while, line after line.\n";

impl FakeMedia {
    /// A file that is an image (a PNG head).
    pub fn put(&self, path: &str, sha: &str) {
        self.put_with_head(path, sha, PNG_HEAD);
    }
    /// A file whose first bytes say neither image nor video, whatever its
    /// name (a text file renamed `.png`).
    pub fn put_not_medium(&self, path: &str, sha: &str) {
        self.put_with_head(path, sha, TEXT_HEAD);
    }
    /// A file with these first bytes.
    pub fn put_with_head(&self, path: &str, sha: &str, head: &[u8]) {
        self.files.lock().unwrap().insert(path.into(), (sha.into(), head.to_vec()));
    }
    /// v2.0.85: an image that is there but above `MEDIA_SIZE_CAP`.
    pub fn put_too_large(&self, path: &str, sha: &str) {
        self.put(path, sha);
        self.too_large.lock().unwrap().push(path.into());
    }
    /// v2.0.85: a file that is there but cannot be read (a read error that is
    /// not « not found »).
    pub fn put_unreadable(&self, path: &str) {
        self.unreadable.lock().unwrap().push(path.into());
    }
    /// Why a read of `path` fails before its bytes, if it does.
    fn read_error(&self, path: &str) -> Option<MediaError> {
        if self.unreadable.lock().unwrap().iter().any(|p| p == path) {
            return Some(MediaError::Unreadable("permission denied".into()));
        }
        if !self.files.lock().unwrap().contains_key(path) {
            return Some(MediaError::Absent);
        }
        None
    }
    fn is_too_large(&self, path: &str) -> bool {
        self.too_large.lock().unwrap().iter().any(|p| p == path)
    }
    /// Every path the service handed this fake, by any method that reads a
    /// file (one entry per call).
    pub fn asked(&self) -> Vec<String> {
        let mut v = self.hashed.lock().unwrap().clone();
        v.extend(self.copied.lock().unwrap().iter().cloned());
        v.extend(self.sniffed.lock().unwrap().iter().map(|(p, _)| p.clone()));
        v.extend(self.verified.lock().unwrap().iter().cloned());
        v
    }
    pub fn hashed(&self) -> Vec<String> {
        self.hashed.lock().unwrap().clone()
    }
    pub fn copied(&self) -> Vec<String> {
        self.copied.lock().unwrap().clone()
    }
    /// (path, max) of every first-bytes read.
    pub fn sniffed(&self) -> Vec<(String, usize)> {
        self.sniffed.lock().unwrap().clone()
    }
    pub fn verified(&self) -> Vec<String> {
        self.verified.lock().unwrap().clone()
    }
    /// How many times `path` was handed to this fake, by any reading method.
    pub fn reads_of(&self, path: &str) -> usize {
        self.asked().iter().filter(|p| *p == path).count()
    }
    /// The (name, sha256) of every medium in the pretend store.
    pub fn in_store(&self) -> Vec<(String, String)> {
        self.stored.lock().unwrap().iter().map(|(n, s, _)| (n.clone(), s.clone())).collect()
    }
    /// The mark the store keeps on the medium named `name`; `None` when it is
    /// not in the store or has no mark.
    pub fn mark_of(&self, name: &str) -> Option<MediaMark> {
        self.stored.lock().unwrap().iter().find(|(n, _, _)| n == name).and_then(|(_, _, m)| *m)
    }
    pub fn forget_asked(&self) {
        self.hashed.lock().unwrap().clear();
        self.copied.lock().unwrap().clear();
        self.sniffed.lock().unwrap().clear();
        self.verified.lock().unwrap().clear();
    }
    fn store(&self, sha: &str, mark: Option<MediaMark>) -> StoredMedium {
        let name = store_name(sha);
        let mut st = self.stored.lock().unwrap();
        if !st.iter().any(|(n, _, _)| *n == name) {
            st.push((name.clone(), sha.to_string(), mark));
        }
        StoredMedium { name, sha256: sha.to_string(), mark }
    }
}

impl MediaFiles for FakeMedia {
    fn sha256(&self, path: &str) -> Option<String> {
        self.hashed.lock().unwrap().push(path.into());
        if self.read_error(path).is_some() {
            return None;
        }
        self.files.lock().unwrap().get(path).map(|(sha, _)| sha.clone())
    }
    fn copy_into_store(&self, outside: &str, mark: Option<MediaMark>) -> Result<StoredMedium, MediaError> {
        self.copied.lock().unwrap().push(outside.into());
        if let Some(e) = self.read_error(outside) {
            return Err(e);
        }
        let (sha, head) = self.files.lock().unwrap().get(outside).cloned().expect("present");
        if !sovereign_docs::is_medium(&head) {
            return Err(MediaError::NotAMedium);
        }
        if self.is_too_large(outside) {
            return Err(MediaError::TooLarge { cap: sovereign_docs::MEDIA_SIZE_CAP });
        }
        Ok(self.store(&sha, mark))
    }
    fn first_bytes(&self, outside: &str, max: usize) -> Result<Vec<u8>, MediaError> {
        self.sniffed.lock().unwrap().push((outside.into(), max));
        if let Some(e) = self.read_error(outside) {
            return Err(e);
        }
        let (_, head) = self.files.lock().unwrap().get(outside).cloned().expect("present");
        Ok(head[..head.len().min(max)].to_vec())
    }
    fn copy_into_store_checked(
        &self,
        outside: &str,
        expected_sha256: &str,
        mark: Option<MediaMark>,
    ) -> Result<StoredMedium, MediaError> {
        self.verified.lock().unwrap().push(outside.into());
        if let Some(e) = self.read_error(outside) {
            return Err(e);
        }
        let (sha, head) = self.files.lock().unwrap().get(outside).cloned().expect("present");
        if !sovereign_docs::is_medium(&head) {
            return Err(MediaError::NotAMedium);
        }
        if self.is_too_large(outside) {
            // Streamed under the cap: it stops there, before any hash.
            return Err(MediaError::TooLarge { cap: sovereign_docs::MEDIA_SIZE_CAP });
        }
        if sha != expected_sha256 {
            return Err(MediaError::HashMismatch);
        }
        Ok(self.store(&sha, mark))
    }
    fn store_name_for(&self, sha256: &str) -> Option<String> {
        self.stored.lock().unwrap().iter().find(|(_, s, _)| s == sha256).map(|(n, _, _)| n.clone())
    }
}

// ------------------------------------------------------------------ stack --

pub struct Stack {
    /// This device's name (recorded on trust decisions and stamps).
    pub device: String,
    pub raw: Arc<SurrealGraphDB>,
    pub faulty: Arc<FaultyDb>,
    pub edb: Arc<EncryptedGraphDB>,
    pub doc_keys: Arc<RwLock<KeyDatabase>>,
    pub clock: Arc<FakeClock>,
    pub events: Arc<RecordingEvents>,
    pub pii: Arc<FakePii>,
    pub policy: Arc<FakePolicy>,
    pub media: Arc<FakeMedia>,
    pub svc: Arc<DocumentService>,
}

/// A file-name stem no other stack in any run can share. The process-wide
/// counter makes it unique within this process whatever the clock does (two
/// parallel tests with the same tag can read the same instant); the pid
/// separates test binaries running at once; the time keeps a later run with a
/// recycled pid off this run's files. Two stacks sharing a stem would share
/// key-database files, and on Windows their atomic writes collide on the
/// same `.tmp` sibling.
fn uniq(tag: &str) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}-{n}-{nanos}-{tag}", std::process::id())
}

impl Stack {
    pub async fn new(tag: &str) -> Stack {
        Stack::with_device(tag, DEVICE).await
    }

    /// A stack whose device has another name (two-device tests).
    pub async fn with_device(tag: &str, device: &str) -> Stack {
        let raw = Arc::new(SurrealGraphDB::new(StorageMode::Memory).await.unwrap());
        raw.connect().await.unwrap();
        raw.init_schema().await.unwrap();
        let faulty = Arc::new(FaultyDb::new(raw.clone() as Arc<dyn GraphDB>));

        let u = uniq(tag);
        let dir = std::env::temp_dir();
        let kdb = |s: &str| {
            Arc::new(RwLock::new(KeyDatabase::new(dir.join(format!("sovereign-docs-{u}-{s}.db")))))
        };
        let doc_keys = kdb("doc");
        let mk = MasterKey::generate();
        let device_key = DeviceKey::derive(&mk, "test-device").unwrap();
        let edb = Arc::new(EncryptedGraphDB::new(
            faulty.clone() as Arc<dyn GraphDB>,
            doc_keys.clone(),
            kdb("msg"),
            kdb("thr"),
            kdb("conv"),
            kdb("con"),
            kdb("shr"),
            Arc::new(Kek::generate()),
            Arc::new(IndexKey::generate()),
            Arc::new(device_key),
        ));

        let clock = Arc::new(FakeClock::new());
        let events = Arc::new(RecordingEvents::default());
        let pii = Arc::new(FakePii::new());
        *pii.db.lock().unwrap() = Some(edb.clone());
        let policy = Arc::new(FakePolicy::new());
        let media = Arc::new(FakeMedia::default());
        let svc = Arc::new(DocumentService::new(ServiceDeps {
            store: edb.clone(),
            commit_policy: policy.clone(),
            pii: pii.clone(),
            media: media.clone(),
            clock: clock.clone(),
            events: events.clone(),
            device: device.into(),
        }));
        Stack { device: device.into(), raw, faulty, edb, doc_keys, clock, events, pii, policy, media, svc }
    }

    /// A new service over the same store (a restart after a crash).
    pub fn restart(&mut self) {
        self.faulty.disarm();
        self.svc = Arc::new(DocumentService::new(ServiceDeps {
            store: self.edb.clone(),
            commit_policy: self.policy.clone(),
            pii: self.pii.clone(),
            media: self.media.clone(),
            clock: self.clock.clone(),
            events: self.events.clone(),
            device: self.device.clone(),
        }));
    }

    // -------------------------------------------------------- creating --

    /// Creates through the service: one text block, owned, written here.
    pub async fn create_text(&self, title: &str, text: &str) -> DocId {
        self.svc
            .create(new_text_doc(title, text))
            .await
            .expect("create")
            .id
    }

    /// Inserts a document **bypassing the service**, with `content` stored
    /// exactly as given — the rows that exist before the service does.
    pub async fn legacy(&self, title: &str, content: &str) -> DocId {
        self.legacy_with(title, content, true).await
    }

    pub async fn legacy_with(&self, title: &str, content: &str, is_owned: bool) -> DocId {
        let mut d = Document::new(title.to_string(), "thread:t1".to_string(), is_owned);
        d.content = content.to_string();
        d.created_at = Utc.with_ymd_and_hms(2025, 3, 1, 9, 0, 0).unwrap();
        d.modified_at = Utc.with_ymd_and_hms(2025, 3, 2, 9, 0, 0).unwrap();
        self.edb.create_document(d).await.unwrap().id_string().unwrap()
    }

    /// An owned row from before an update, **bypassing the service**, whose
    /// current state was written on `author` (C-MEDIA's migration pass tells a
    /// state written here from one written elsewhere by `author_device`).
    pub async fn legacy_authored(&self, title: &str, content: &str, author: &str) -> DocId {
        let mut d = Document::new(title.to_string(), "thread:t1".to_string(), true);
        d.content = content.to_string();
        d.created_at = Utc.with_ymd_and_hms(2025, 3, 1, 9, 0, 0).unwrap();
        d.modified_at = Utc.with_ymd_and_hms(2025, 3, 2, 9, 0, 0).unwrap();
        d.author_device = Some(author.to_string());
        self.edb.create_document(d).await.unwrap().id_string().unwrap()
    }

    // ---------------------------------------------------------- reading --

    /// The stored (decrypted) title and content, read under the service.
    pub async fn stored(&self, id: &str) -> (String, String) {
        let d = self.edb.get_document(id).await.expect("stored doc");
        (d.title, d.content)
    }

    pub async fn stored_doc(&self, id: &str) -> Document {
        self.edb.get_document(id).await.expect("stored doc")
    }

    /// Commits of a document, newest first, decrypted.
    pub async fn commits(&self, id: &str) -> Vec<Commit> {
        self.edb.list_document_commits(id).await.unwrap_or_default()
    }

    /// (title, content) of every commit of `id`.
    pub async fn committed_states(&self, id: &str) -> Vec<(String, String)> {
        self.commits(id)
            .await
            .into_iter()
            .map(|c| (c.snapshot.title, c.snapshot.content))
            .collect()
    }

    /// Commits as stored at rest (signature as written).
    pub async fn raw_commits(&self, id: &str) -> Vec<Commit> {
        self.raw.list_document_commits(id).await.unwrap_or_default()
    }

    pub async fn revision(&self, id: &str) -> Revision {
        self.svc.read(id).await.expect("read").revision
    }

    // ---------------------------------------------------------- writing --

    /// Saves `content` (whole v2) from editor session `session`.
    pub async fn editor_save(
        &self,
        id: &str,
        session: u64,
        content: String,
    ) -> Result<sovereign_docs::SaveOutcome, sovereign_docs::DocError> {
        let base = self.revision(id).await;
        self.svc
            .save(id, Edit {
                base,
                title: None,
                content: Some(EditContent::V2(content)),
                origin: Origin::Editor { session: EditSession(session) },
                explicit: false,
                pasted_tokens: vec![],
            })
            .await
    }

    /// Saves plain text as the assistant.
    pub async fn assistant_save(
        &self,
        id: &str,
        text: &str,
    ) -> Result<sovereign_docs::SaveOutcome, sovereign_docs::DocError> {
        let base = self.revision(id).await;
        self.svc
            .save(id, Edit {
                base,
                title: None,
                content: Some(EditContent::PlainText(text.into())),
                origin: Origin::Assistant,
                explicit: false,
                pasted_tokens: vec![],
            })
            .await
    }
}

// ------------------------------------------------------------ fixtures --

pub fn new_text_doc(title: &str, text: &str) -> NewDocument {
    NewDocument {
        title: title.into(),
        thread_id: "thread:t1".into(),
        content: NewContent::Text(text.into()),
        trust: Trust::Owned,
        provenance: Provenance::Written,
        origin: Origin::Editor { session: EditSession(1) },
        created_at: None,
        modified_at: None,
    }
}

/// A block id as stored: 32 hex digits.
pub fn hex(n: u8) -> String {
    format!("{:032x}", n as u128 + 0x1000)
}

pub fn bid(n: u8) -> BlockId {
    let v = n as u128 + 0x1000;
    BlockId(v.to_be_bytes())
}

pub fn text_block(n: u8, text: &str) -> Value {
    json!({"type": "text", "id": hex(n), "markup": "markdown", "text": text})
}

pub fn image_block(n: u8, path: &str, sha: Option<&str>) -> Value {
    let mut r = json!({"path": path});
    if let Some(s) = sha {
        r["sha256"] = json!(s);
    }
    json!({"type": "image", "id": hex(n), "ref": r, "alt": "a cat", "caption": "c"})
}

/// A block type no version knows, with fields of its own.
pub fn unknown_block(n: u8) -> Value {
    json!({"type": "x-chart", "id": hex(n), "series": [1, 2.50, -3], "opts": {"z": null}})
}

pub fn v2(blocks: Vec<Value>) -> String {
    serde_json::to_string(&json!({"format": "sovereign.doc", "v": 2, "blocks": blocks})).unwrap()
}

/// Parses stored content and asserts it is v2. Returns its blocks.
pub fn assert_v2(stored: &str) -> Vec<Value> {
    let v: Value = serde_json::from_str(stored)
        .unwrap_or_else(|e| panic!("stored content is not JSON ({e}): {} bytes", stored.len()));
    assert_eq!(v["format"], json!("sovereign.doc"), "format");
    assert_eq!(v["v"], json!(2), "version");
    let blocks = v["blocks"].as_array().expect("blocks array").clone();
    for b in &blocks {
        assert!(b["type"].is_string(), "every block has a type");
        let id = b["id"].as_str().expect("every block has an id");
        assert_eq!(id.len(), 32, "block id is 32 hex digits");
        assert!(id.bytes().all(|c| c.is_ascii_hexdigit()));
    }
    blocks
}

/// The texts of the text blocks of stored v2 content.
pub fn texts(stored: &str) -> Vec<String> {
    assert_v2(stored)
        .iter()
        .filter(|b| b["type"] == "text")
        .map(|b| b["text"].as_str().unwrap_or_default().to_string())
        .collect()
}

pub fn types_of(stored: &str) -> Vec<String> {
    assert_v2(stored).iter().map(|b| b["type"].as_str().unwrap().to_string()).collect()
}

/// A discovered personal-data record whose sources are the given documents.
pub async fn new_record(db: &Arc<EncryptedGraphDB>, docs: &[&str]) -> String {
    record_with(db, docs, false).await
}

/// A vault entry (stored by the user) whose sources are the given documents.
pub async fn new_vault_record(db: &Arc<EncryptedGraphDB>, docs: &[&str]) -> String {
    record_with(db, docs, true).await
}

async fn record_with(db: &Arc<EncryptedGraphDB>, docs: &[&str], stored_secret: bool) -> String {
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let rec = PiiRecord {
        id: None,
        kind: PiiKind::Email,
        value_encrypted: format!("ct-{n}"),
        value_nonce: "nonce".into(),
        label: None,
        entity_id: None,
        stored_secret,
        confidence: 1.0,
        sources: docs
            .iter()
            .map(|d| SourceRef {
                source_kind: SourceKind::Document,
                source_id: d.to_string(),
                span_start: 0,
                span_end: 0,
            })
            .collect(),
        discovered_at: Utc::now(),
        last_revealed_at: None,
        use_count: 0,
        review_state: ReviewState::Confirmed,
        review_stamp: None,
        deleted_at: None,
    };
    db.create_pii_record(rec).await.unwrap().id_string().unwrap()
}

/// The document ids a record lists among its sources.
pub async fn sources_of(db: &Arc<EncryptedGraphDB>, rec: &str) -> Vec<String> {
    db.get_pii_record(rec)
        .await
        .map(|r| r.sources.into_iter().map(|s| s.source_id).collect())
        .unwrap_or_default()
}

pub fn token(rec: &str) -> String {
    format!("[pii:{rec}]")
}
