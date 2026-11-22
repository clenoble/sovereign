//! The phone's History that cannot be read (item C5 of v0.0.13; spec ch. 03,
//! *What cannot be read is never shown as empty*): the layer under the
//! commands `list_commits` and its count for the sync alerts. Written before
//! the code, by another author: whoever implements it makes these pass
//! without editing this file; a test that looks wrong is reported, not worked
//! around. The Svelte side is tested by vitest
//! (`components/mobile/MobileHistory.c5.svelte.test.ts`).
//!
//! What was decided (2026-11-18, the Fable maille by delegation of Céline):
//! - the versions that cannot be read are said, never shown as empty: no list
//!   and the sentence of `sovereign-docs` (`COULD_NOT_READ_HISTORY`) in
//!   `could_not_read`; `None` when they read;
//! - one version that cannot be read stays in the list, in its place and at
//!   its time; its title and preview are `VERSION_COULD_NOT_BE_READ`, never
//!   its ciphertext;
//! - the backend counts, for the session, the documents whose history it
//!   could not read (a whole list, or a version in it); a document leaves the
//!   count at its next good read; nothing is stored;
//! - a locked session stays a refusal (`Err`), never « could not be read ».
//!
//! **The seam** (the test author's « types and signatures » commit):
//! - `tauri_commands::documents::commits_listing(docs, doc_id) ->
//!   Result<CommitListDto, String>`, `CommitListDto { commits, could_not_read }`,
//!   `CommitSummaryDto::could_not_read`;
//! - `histories_not_read_listing(docs) -> Result<HistoriesNotReadDto, String>`,
//!   `HistoriesNotReadDto { count, sentence }`, the sentence
//!   `sovereign_docs::histories_not_read_sentence(count)`;
//! - `docs` is what `AppState::docs` gives: the session's document service,
//!   or why there is none.
//!
//! Stores: as in `could_not_check_tests`, a real `EncryptedGraphDB` over the
//! document service suite's `FaultyDb` over an in-memory SurrealDB. The whole
//! list cannot be read when the store's read under it fails
//! (`FaultyDb::fail_named`). One version cannot be read for real: it is
//! written under the encrypted layer with bytes that do not decrypt where its
//! title and content belong. The spec's shared assertion is
//! `sovereign-docs/tests/common/counted_and_shown.rs`.

use std::sync::Arc;

use chrono::{Duration, Utc};
use sovereign_ai::docs_host::{build_document_service, new_user_document, testing};
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::IndexKey;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{Commit, DocumentSnapshot, Thread};
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{
    histories_not_read_sentence, run_unlock_migration, DocumentService, COULD_NOT_READ_HISTORY,
    VERSION_COULD_NOT_BE_READ,
};

use crate::tauri_commands::documents::{
    commits_listing, histories_not_read_listing, CommitListDto, HistoriesNotReadDto,
};
use crate::tauri_commands::CommitSummaryDto;

#[path = "../../sovereign-docs/tests/common/faulty.rs"]
#[allow(dead_code, clippy::all)]
mod faulty;
use faulty::FaultyDb;

#[path = "../../sovereign-docs/tests/common/counted_and_shown.rs"]
mod counted_and_shown;
use counted_and_shown::assert_counted_and_shown;

/// base64("not-a-real-ciphertext") and 24 zero bytes: valid base64, not a
/// ciphertext of anything (DB-M1's test).
const JUNK_CT: &str = "bm90LWEtcmVhbC1jaXBoZXJ0ZXh0";
const JUNK_NONCE: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// The store read under a document's versions.
const VERSIONS_READ: &str = "list_document_commits";

/// What `AppState::docs` says while the workspace is locked.
const LOCKED: &str = "The workspace is locked: log in to change documents.";

/// What the phone says when a document has no versions: never in place of
/// versions that could not be read.
const NO_VERSIONS: &str = "No saved versions yet";

// ======================================================================= fixtures

struct S {
    raw: Arc<SurrealGraphDB>,
    faulty: Arc<FaultyDb>,
    store: Arc<EncryptedGraphDB>,
    docs: Arc<DocumentService>,
}

/// The phone's service over an encrypted store over `FaultyDb`, after the
/// one-time conversion (what an unlock runs).
async fn session(tag: &str) -> S {
    let raw = Arc::new(SurrealGraphDB::new(StorageMode::Memory).await.expect("memory store"));
    raw.connect().await.expect("connect");
    raw.init_schema().await.expect("schema");
    let faulty = Arc::new(FaultyDb::new(raw.clone() as Arc<dyn GraphDB>));
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir();
    let kdb = |s: &str| {
        Arc::new(tokio::sync::RwLock::new(KeyDatabase::new(
            dir.join(format!("c5-history-{}-{nanos}-{seq}-{tag}-{s}.db", std::process::id())),
        )))
    };
    let mk = MasterKey::generate();
    let store = Arc::new(EncryptedGraphDB::new(
        faulty.clone() as Arc<dyn GraphDB>,
        kdb("doc"),
        kdb("msg"),
        kdb("thr"),
        kdb("conv"),
        kdb("con"),
        kdb("shr"),
        Arc::new(Kek::generate()),
        Arc::new(IndexKey::generate()),
        Arc::new(DeviceKey::derive(&mk, "test-device").expect("device key")),
    ));
    let docs = Arc::new(build_document_service(store.clone(), Some(testing::account_key()), "phone".into()));
    run_unlock_migration(&docs).await.expect("the conversion runs");
    S { raw, faulty, store, docs }
}

impl S {
    /// A document with one version that reads; its id.
    async fn document(&self, title: &str) -> String {
        let t = self.store.create_thread(Thread::new("Lane".into(), String::new())).await.unwrap().id_string().unwrap();
        let id = self.docs.create(new_user_document(title, &t, "some text")).await.unwrap().id;
        self.store.commit_document(&id, "Saved").await.expect("a version that reads");
        id
    }

    /// Writes, under the encrypted layer, a version of `doc_id` whose title
    /// and content do not decrypt, a minute after now (so it is the newest);
    /// its id and its time.
    async fn unreadable_version(&self, doc_id: &str) -> (String, chrono::DateTime<Utc>) {
        let when = Utc::now() + Duration::minutes(1);
        let c = self
            .raw
            .insert_commit(Commit {
                id: None,
                document_id: doc_id.to_string(),
                parent_commit: None,
                message: "Saved".into(),
                timestamp: when,
                snapshot: DocumentSnapshot {
                    document_id: doc_id.to_string(),
                    title: JUNK_CT.into(),
                    content: JUNK_CT.into(),
                    content_nonce: Some(JUNK_NONCE.into()),
                    title_nonce: Some(JUNK_NONCE.into()),
                    ..Default::default()
                },
                signature: None,
            })
            .await
            .expect("a version written at rest");
        (c.id_string().expect("its id"), when)
    }

    fn versions_cannot_be_read(&self) {
        self.faulty.fail_named(&[VERSIONS_READ]);
    }

    fn versions_read_again(&self) {
        self.faulty.stop_failing_named();
    }

    async fn listing(&self, doc_id: &str) -> CommitListDto {
        commits_listing(Ok(self.docs.clone()), doc_id).await.expect("not a refusal: the session is open")
    }

    async fn count(&self) -> HistoriesNotReadDto {
        histories_not_read_listing(Ok(self.docs.clone())).await.expect("not a refusal: the session is open")
    }
}

/// Every text the listing hands the frontend to show: its sentence, then each
/// version's title, preview and sentence.
fn shown(l: &CommitListDto) -> Vec<String> {
    l.could_not_read
        .iter()
        .cloned()
        .chain(l.commits.iter().flat_map(|c| {
            [c.snapshot_title.clone(), c.snapshot_preview.clone()].into_iter().chain(c.could_not_read.iter().cloned())
        }))
        .collect()
}

// ======================================================= the whole list

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn versions_that_cannot_be_read_come_as_the_sentence_and_no_list() {
    let s = session("whole").await;
    let id = s.document("Notes").await;
    s.versions_cannot_be_read();
    let l = s.listing(&id).await;
    assert!(l.commits.is_empty(), "no list when it could not be read: {l:?}");
    assert_eq!(l.could_not_read.as_deref(), Some(COULD_NOT_READ_HISTORY), "the sentence in its place: {l:?}");
    let n = s.count().await;
    let mut said = shown(&l);
    said.extend(n.sentence.iter().cloned());
    assert_counted_and_shown("a document's History that could not be read", n.count, &said, COULD_NOT_READ_HISTORY, &[NO_VERSIONS]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn versions_that_read_come_with_no_sentence() {
    let s = session("reads").await;
    let id = s.document("Notes").await;
    let l = s.listing(&id).await;
    assert!(!l.commits.is_empty(), "the version made by the fixture is listed: {l:?}");
    assert_eq!(l.could_not_read, None, "nothing to say when they read: {l:?}");
    for c in &l.commits {
        assert_eq!(c.could_not_read, None, "a version that reads has no sentence: {c:?}");
    }
    let n = s.count().await;
    assert_eq!((n.count, n.sentence.as_deref()), (0, None), "nothing counted when every read succeeds: {n:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_locked_session_is_a_refusal_never_could_not_be_read() {
    let r = commits_listing(Err(LOCKED.to_string()), "document:any").await;
    match r {
        Err(e) => assert_eq!(e, LOCKED, "the refusal as it came"),
        Ok(l) => panic!("a locked session is refused, not a list or a sentence: {l:?}"),
    }
    let r = histories_not_read_listing(Err(LOCKED.to_string())).await;
    match r {
        Err(e) => assert_eq!(e, LOCKED, "the refusal as it came"),
        Ok(n) => panic!("a locked session is refused, not a count: {n:?}"),
    }
}

// ======================================================= one version

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_version_that_cannot_be_read_stays_in_its_place_said_never_its_ciphertext() {
    let s = session("one").await;
    let id = s.document("Notes").await;
    let (bad, when) = s.unreadable_version(&id).await;
    let l = s.listing(&id).await;
    assert_eq!(l.could_not_read, None, "the list itself read: {l:?}");
    assert!(l.commits.len() >= 2, "both versions are listed, the one that reads and the other: {l:?}");

    let first = &l.commits[0];
    assert_eq!(first.id, bad, "the version that cannot be read keeps its place, the newest here: {l:?}");
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(&first.timestamp).map(|t| t.timestamp()).ok(),
        Some(when.timestamp()),
        "and its time: {first:?}"
    );
    assert_eq!(first.could_not_read.as_deref(), Some(VERSION_COULD_NOT_BE_READ), "said by its sentence: {first:?}");
    assert_eq!((first.snapshot_title.as_str(), first.snapshot_preview.as_str()), ("", ""), "no title, no preview: {first:?}");
    for text in shown(&l) {
        assert!(!text.contains(JUNK_CT), "never the ciphertext: {text:?} in {l:?}");
    }
    for c in &l.commits[1..] {
        assert_eq!(c.could_not_read, None, "the other versions read as before: {c:?}");
    }

    let n = s.count().await;
    let mut said = shown(&l);
    said.extend(n.sentence.iter().cloned());
    assert_counted_and_shown("one version that could not be read", n.count, &said, VERSION_COULD_NOT_BE_READ, &[NO_VERSIONS]);
    assert_eq!(n.count, 1, "the document is counted: {n:?}");
}

// ======================================================= the count

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_count_is_of_documents_and_says_how_many() {
    let s = session("count").await;
    let a = s.document("First").await;
    let b = s.document("Second").await;
    s.versions_cannot_be_read();
    s.listing(&a).await;
    s.listing(&a).await;
    let n = s.count().await;
    assert_eq!(n.count, 1, "one document, read twice, is counted once: {n:?}");
    assert_eq!(n.sentence, Some(histories_not_read_sentence(1)), "in the backend's words: {n:?}");
    s.listing(&b).await;
    let n = s.count().await;
    assert_eq!(n.count, 2, "two documents: {n:?}");
    assert_eq!(n.sentence, Some(histories_not_read_sentence(2)), "in the backend's words: {n:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_document_leaves_the_count_at_its_next_good_read() {
    let s = session("clears").await;
    let id = s.document("Notes").await;
    s.versions_cannot_be_read();
    s.listing(&id).await;
    assert_eq!(s.count().await.count, 1, "counted after the failed read");
    s.versions_read_again();
    let l = s.listing(&id).await;
    assert_eq!(l.could_not_read, None, "fixture: this read succeeds: {l:?}");
    let n = s.count().await;
    assert_eq!((n.count, n.sentence.as_deref()), (0, None), "no longer counted after a good read: {n:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_read_that_still_finds_a_version_it_cannot_read_is_not_a_good_read() {
    let s = session("still").await;
    let id = s.document("Notes").await;
    s.unreadable_version(&id).await;
    s.listing(&id).await;
    s.listing(&id).await;
    let n = s.count().await;
    assert_eq!(n.count, 1, "still counted, once: {n:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_count_is_the_sessions_and_nothing_of_it_is_stored() {
    let s = session("memory").await;
    let id = s.document("Notes").await;
    s.versions_cannot_be_read();
    s.listing(&id).await;
    assert_eq!(s.count().await.count, 1, "counted in this session");
    s.versions_read_again();
    // Another session over the same store: what an unlock after a restart builds.
    let next = Arc::new(build_document_service(s.store.clone(), Some(testing::account_key()), "phone".into()));
    let n = histories_not_read_listing(Ok(next)).await.expect("not a refusal");
    assert_eq!((n.count, n.sentence.as_deref()), (0, None), "a new session starts with nothing counted: {n:?}");
}

// ======================================================= the shapes

/// The objects travel with the field names `frontend/src/lib/api/commands.ts`
/// reads (`CommitListDto`, `CommitSummary`, `HistoriesNotReadDto`).
#[test]
fn the_objects_travel_with_the_frontends_field_names() {
    let version = CommitSummaryDto {
        id: "commit:a".into(),
        message: "m".into(),
        timestamp: "t".into(),
        snapshot_title: String::new(),
        snapshot_preview: String::new(),
        could_not_read: Some("x".into()),
    };
    assert_eq!(
        serde_json::to_value(&version).unwrap(),
        serde_json::json!({ "id": "commit:a", "message": "m", "timestamp": "t", "snapshot_title": "", "snapshot_preview": "", "could_not_read": "x" })
    );
    let list = CommitListDto { commits: vec![], could_not_read: Some("y".into()) };
    assert_eq!(serde_json::to_value(&list).unwrap(), serde_json::json!({ "commits": [], "could_not_read": "y" }));
    let n = HistoriesNotReadDto { count: 0, sentence: None };
    assert_eq!(serde_json::to_value(&n).unwrap(), serde_json::json!({ "count": 0, "sentence": null }));
}
