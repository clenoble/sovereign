//! C-SWEEP between two of her devices (spec ch. 09, v2.0.111): what travels by
//! sync, in-process, with no node listening. Each device is an encrypted
//! in-memory store with its document service (the regex hook, her account key,
//! which paired devices share) and its row sync service (`SyncService`). A sync
//! round exchanges every non-document table both ways, as the node does
//! (`compute_all_row_diffs`, `get_rows`, `apply_rows`); a document travels as
//! its stored state (`stored_state`, `apply_wire`).
//!
//! - **SW1:** a state is never lowered by another device's ingest; a confirmed
//!   record that arrives after its document replaces the suspected one the
//!   hook wrote; one that arrives first keeps its state.
//! - **SW5:** the review state and the dismissals travel (Céline, 2026-11-17:
//!   « Voyage, + téléphone complet »); the contract's test, on a second device:
//!   she dismisses a value, the other device edits another paragraph and saves;
//!   the value is not back in the queue and reaches the model as written.
//! - **SW3:** the incoming-change analysis masks with the document's id
//!   (`ModelJudge::judge_document`).
//!
//! Here because the shell links both `sovereign-ai` (with its `p2p` feature)
//! and `sovereign-p2p`, which do not link each other in their own tests.
//!
//! Written before the code by the test author of lot LS(a); whoever implements
//! does not edit this file.

use std::sync::{Arc, Mutex};

use sovereign_ai::docs_host::testing::{account_key, encrypted_memory_store};
use sovereign_ai::docs_host::{build_document_service, create, new_edit_session, new_user_document};
use sovereign_ai::pii::review::PiiReview;
use sovereign_ai::pii::signal;
use sovereign_crypto::account_key::AccountKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{PiiKind, PiiRecord, ReviewState, SourceKind};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{BlockId, DocumentService, DocumentV2, Edit, EditContent, NewContent, Origin};
use sovereign_p2p::libp2p::identity::Keypair;
use sovereign_p2p::libp2p::PeerId;
use sovereign_p2p::{SyncService, VersionStore};

const IBAN: &str = "CH93 0076 2011 6238 5295 7";
const IBAN2: &str = "CH56 0483 5012 3456 7800 9";
const PAIR_KEY: [u8; 32] = [0x5a; 32];

struct Device {
    store: Arc<EncryptedGraphDB>,
    docs: Arc<DocumentService>,
    unkeyed: Arc<DocumentService>,
    key: Arc<AccountKey>,
    sync: SyncService,
    peer: PeerId,
}

async fn device(tag: &str, seed: u8) -> Device {
    std::env::set_var("SOVEREIGN_DATA_DIR", std::env::temp_dir().join(format!("sov-csweep-two-{}", std::process::id())));
    let store = encrypted_memory_store(tag).await;
    let key = account_key();
    let keypair = Keypair::ed25519_from_bytes([seed; 32]).unwrap();
    let peer = keypair.public().to_peer_id();
    let sync = SyncService::new(store.clone() as Arc<dyn GraphDB>, peer.to_string(), [7u8; 32], keypair, VersionStore::ephemeral());
    Device {
        docs: Arc::new(build_document_service(store.clone(), Some(key.clone()), peer.to_string())),
        unkeyed: Arc::new(build_document_service(store.clone(), None, peer.to_string())),
        store,
        key,
        sync,
        peer,
    }
}

/// Two of her paired devices.
async fn pair(tag: &str) -> (Device, Device) {
    let a = device(&format!("{tag}-a"), 0xA1).await;
    let b = device(&format!("{tag}-b"), 0xB2).await;
    a.sync.add_pair_key(b.peer.to_string(), PAIR_KEY);
    b.sync.add_pair_key(a.peer.to_string(), PAIR_KEY);
    (a, b)
}

/// One round of row sync between `a` and `b`, both ways, every table.
async fn round(a: &Device, b: &Device) {
    let ma = a.sync.build_manifest().await.unwrap();
    let mb = b.sync.build_manifest().await.unwrap();
    for (table, diff) in sovereign_p2p::sync_engine::compute_all_row_diffs(&ma, &mb) {
        if !diff.need_from_remote.is_empty() {
            let rows = b.sync.get_rows(table, &diff.need_from_remote, &a.peer).await.unwrap();
            a.sync.apply_rows(table, rows, &b.peer).await.unwrap();
        }
        if !diff.push_to_remote.is_empty() {
            let rows = a.sync.get_rows(table, &diff.push_to_remote, &b.peer).await.unwrap();
            b.sync.apply_rows(table, rows, &a.peer).await.unwrap();
        }
    }
}

/// The document `id` as `from` stores it, applied on `to`.
async fn send_doc(from: &DocumentService, to: &DocumentService, id: &str) {
    let st = from.stored_state(id).await.unwrap();
    to.apply_wire(st.document, st.meta).await.unwrap();
}

async fn ibans(store: &EncryptedGraphDB, doc: &str) -> Vec<PiiRecord> {
    store
        .list_pii_records(None, None, Some(false))
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.deleted_at.is_none() && r.kind == PiiKind::Iban)
        .filter(|r| r.sources.iter().any(|s| s.source_kind == SourceKind::Document && s.source_id == doc))
        .collect()
}

async fn state_on(store: &EncryptedGraphDB, record: &str) -> ReviewState {
    store.get_pii_record(record).await.unwrap().review_state
}

/// A document not analysed on `a` (no key), its value raw, and its id.
async fn raw_on(a: &Device) -> String {
    create(&a.unkeyed, new_user_document("Bank", "thread:t", format!("pay to {IBAN}"))).await.unwrap().id
}

fn review(d: &Device) -> PiiReview {
    PiiReview::new(d.docs.clone(), d.key.clone())
}

// ======================================================================= SW5

/// SW5: the review state travels: a confirmation taken back on one device is
/// taken back on the other, and a confirmation given again travels too.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw5_the_review_state_travels() {
    let (a, b) = pair("sw5-state").await;
    let id = create(&a.docs, new_user_document("Bank", "thread:t", format!("pay to {IBAN}"))).await.unwrap().id;
    let rid = ibans(&a.store, &id).await[0].id_string().unwrap();
    send_doc(&a.docs, &b.docs, &id).await;
    round(&a, &b).await;
    assert_eq!(state_on(&b.store, &rid).await, ReviewState::Confirmed, "test setup: the record travelled");

    review(&a).take_back_confirmation(&[rid.clone()]).await.unwrap();
    round(&a, &b).await;
    assert_eq!(state_on(&b.store, &rid).await, ReviewState::Unreviewed, "the take-back travelled");

    review(&a).confirm(&[rid.clone()]).await.unwrap();
    round(&a, &b).await;
    assert_eq!(state_on(&b.store, &rid).await, ReviewState::Confirmed, "the confirmation travelled");
}

/// SW5, the contract's test on a second device: she dismisses a value on one
/// device; the other edits another paragraph and saves. The value stands as
/// written there, is not back in its queue, has no new record, and reaches
/// that device's model as written.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw5_a_dismissal_travels_and_holds_on_the_second_device() {
    let (a, b) = pair("sw5-dismissal").await;
    let mut doc = DocumentV2::from_text(&format!("pay to {IBAN}"), "two-devices");
    let second = BlockId::derived("two-devices-second", 1);
    doc.insert_text_block(1, second, "a note").unwrap();
    let mut new = new_user_document("Bank", "thread:t", "");
    new.content = NewContent::V2(doc.to_json_string());
    let id = create(&a.docs, new).await.unwrap().id;
    send_doc(&a.docs, &b.docs, &id).await;
    round(&a, &b).await;

    let rid = ibans(&a.store, &id).await[0].id_string().unwrap();
    review(&a).dismiss(&[rid.clone()]).await.unwrap();
    send_doc(&a.docs, &b.docs, &id).await;
    round(&a, &b).await;
    assert_eq!(state_on(&b.store, &rid).await, ReviewState::Dismissed, "the dismissal travelled");

    let base = b.docs.read(&id).await.unwrap().revision;
    b.docs
        .save(&id, Edit {
            base,
            title: None,
            content: Some(EditContent::TextBlocks(vec![(second, "a longer note".into())])),
            origin: Origin::Editor { session: new_edit_session() },
            explicit: false,
            pasted_tokens: vec![],
        })
        .await
        .unwrap();
    assert!(b.docs.read(&id).await.unwrap().plain_text().contains(IBAN), "the dismissed value stands as written");
    let on_b = ibans(&b.store, &id).await;
    assert_eq!(on_b.len(), 1, "no new record for a dismissed value");
    assert_eq!(on_b[0].review_state, ReviewState::Dismissed);
    assert_eq!(signal::waiting(b.store.as_ref()).await.unwrap(), 0, "not back in the queue");

    // The second device's model reads the document as written.
    let prompts: Arc<Mutex<Vec<String>>> = Arc::default();
    let p = prompts.clone();
    let script: sovereign_ai::llm::async_backend::FakeGenerate = Arc::new(move |prompt: &str, _| {
        let mut all = p.lock().unwrap();
        all.push(prompt.to_string());
        Ok(match all.len() {
            1 => serde_json::json!({ "action": "chat", "target": null, "confidence": 0.95 }).to_string(),
            2 => serde_json::json!({ "name": "get_document", "arguments": { "title": "Bank" } }).to_string(),
            _ => "Read it.".to_string(),
        })
    });
    let (tx, _rx) = std::sync::mpsc::channel();
    let dir = std::env::temp_dir().join(format!("sov-csweep-two-orch-{}", std::process::id()));
    let paths = sovereign_ai::OrchestratorStatePaths::for_persona(&dir, sovereign_core::auth::PersonaKind::Primary);
    let orch = sovereign_ai::Orchestrator::for_host_tests_with_model(b.store.clone() as Arc<dyn GraphDB>, tx, paths, 4096, script);
    orch.set_pii_account_key(b.key.clone());
    orch.set_document_service(b.docs.clone());
    orch.handle_chat("What does Bank say?").await.unwrap();
    let all = prompts.lock().unwrap().clone();
    assert!(all.len() >= 3, "the document was read: {} calls", all.len());
    assert!(all.iter().any(|p| p.contains(&format!("pay to {IBAN}"))), "her dismissal reaches the model as written");
}

// ======================================================================= SW1

/// SW1: a document and its records arrive in either order. Here the document
/// first: the receiving hook suspects the value (with the same derived record
/// id, SW2); the confirmed record arriving after it replaces that state.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_a_confirmed_record_after_its_document_replaces_the_suspected_state() {
    let (a, b) = pair("sw1-doc-first").await;
    let id = raw_on(&a).await;
    send_doc(&a.unkeyed, &b.docs, &id).await;
    let on_b = ibans(&b.store, &id).await;
    assert_eq!(on_b.len(), 1);
    assert_eq!(on_b[0].review_state, ReviewState::Unreviewed, "content no device analysed is suspected on arrival (SW1)");

    assert!(a.docs.sweep(&id).await.unwrap().changed);
    let rid = ibans(&a.store, &id).await[0].id_string().unwrap();
    assert_eq!(Some(rid.clone()), on_b[0].id_string(), "one identity for the finding on both devices (SW2)");
    review(&a).confirm(&[rid.clone()]).await.unwrap();
    round(&a, &b).await;
    assert_eq!(state_on(&b.store, &rid).await, ReviewState::Confirmed);
    assert_eq!(ibans(&b.store, &id).await.len(), 1);
}

/// SW1: the record first, then its document: the record keeps its state.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_a_confirmed_record_before_its_document_keeps_its_state() {
    let (a, b) = pair("sw1-record-first").await;
    let id = raw_on(&a).await;
    assert!(a.docs.sweep(&id).await.unwrap().changed);
    let rid = ibans(&a.store, &id).await[0].id_string().unwrap();
    review(&a).confirm(&[rid.clone()]).await.unwrap();
    round(&a, &b).await;
    send_doc(&a.docs, &b.docs, &id).await;
    assert_eq!(state_on(&b.store, &rid).await, ReviewState::Confirmed);
    assert_eq!(ibans(&b.store, &id).await.len(), 1);
}

/// SW1: never lowered, in either direction: one device confirmed the value;
/// the other's sweep suspected the same finding; after a round both hold it
/// confirmed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw1_another_devices_ingest_never_lowers_a_state() {
    let (a, b) = pair("sw1-never-lowered").await;
    let id = raw_on(&a).await;
    send_doc(&a.unkeyed, &b.unkeyed, &id).await;
    assert!(b.docs.sweep(&id).await.unwrap().changed);
    let rid = ibans(&b.store, &id).await[0].id_string().unwrap();
    review(&b).confirm(&[rid.clone()]).await.unwrap();
    assert!(a.docs.sweep(&id).await.unwrap().changed);
    assert_eq!(state_on(&a.store, &rid).await, ReviewState::Unreviewed, "test setup: suspected by a's sweep");

    round(&a, &b).await;
    assert_eq!(state_on(&b.store, &rid).await, ReviewState::Confirmed, "a's ingest did not lower b's state");
    assert_eq!(state_on(&a.store, &rid).await, ReviewState::Confirmed, "b's confirmation replaced a's suspicion");
}

// ======================================================================= SW3

/// The prompts a fake router model received, and a judge over it.
fn judge_over(store: &Arc<EncryptedGraphDB>, key: Arc<AccountKey>, docs: Arc<DocumentService>) -> (sovereign_ai::peer_audit::OrchestratorJudge, Arc<Mutex<Vec<String>>>) {
    let prompts: Arc<Mutex<Vec<String>>> = Arc::default();
    let p = prompts.clone();
    let script: sovereign_ai::llm::async_backend::FakeGenerate = Arc::new(move |prompt: &str, _| {
        p.lock().unwrap().push(prompt.to_string());
        Ok(r#"{"risk": "low", "reasons": []}"#.to_string())
    });
    let (tx, _rx) = std::sync::mpsc::channel();
    let dir = std::env::temp_dir().join(format!("sov-csweep-judge-{}", std::process::id()));
    let paths = sovereign_ai::OrchestratorStatePaths::for_persona(&dir, sovereign_core::auth::PersonaKind::Primary);
    let orch = sovereign_ai::Orchestrator::for_host_tests_with_model(store.clone() as Arc<dyn GraphDB>, tx, paths, 4096, script);
    orch.set_pii_account_key(key);
    orch.set_document_service(docs);
    (sovereign_ai::peer_audit::OrchestratorJudge::with_orchestrator(Arc::new(orch)), prompts)
}

/// SW3: the incoming-change analysis reads masked: a value in the texts
/// reaches its model as its label.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw3_the_incoming_change_analysis_reads_masked() {
    use sovereign_p2p::wire::ModelJudge;
    let (a, _b) = pair("sw3-judge").await;
    let id = raw_on(&a).await;
    let (judge, prompts) = judge_over(&a.store, a.key.clone(), a.docs.clone());
    judge.judge_document(&id, "pay later", &format!("send to {IBAN2} now")).await;
    let all = prompts.lock().unwrap().clone();
    assert!(!all.is_empty(), "the model was asked");
    assert!(all.iter().all(|p| !p.contains(IBAN2)), "the value reached the model raw");
    assert!(all.iter().any(|p| p.contains("[IBAN]")), "the value as its label");
}

/// SW3: with the document's id, her dismissal of that document reaches the
/// analysis as written, every other value as its label.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sw3_the_incoming_change_analysis_reads_a_dismissal_as_written() {
    use sovereign_p2p::wire::ModelJudge;
    let (a, _b) = pair("sw3-judge-dismissed").await;
    let id = create(&a.docs, new_user_document("Bank", "thread:t", format!("pay to {IBAN}"))).await.unwrap().id;
    let rid = ibans(&a.store, &id).await[0].id_string().unwrap();
    review(&a).dismiss(&[rid]).await.unwrap();
    let (judge, prompts) = judge_over(&a.store, a.key.clone(), a.docs.clone());
    judge.judge_document(&id, &format!("pay to {IBAN}"), &format!("pay to {IBAN} or {IBAN2}")).await;
    let all = prompts.lock().unwrap().clone();
    assert!(all.iter().any(|p| p.contains(IBAN)), "her dismissal reaches the model as written");
    assert!(all.iter().all(|p| !p.contains(IBAN2)), "an undismissed value never raw");
}
