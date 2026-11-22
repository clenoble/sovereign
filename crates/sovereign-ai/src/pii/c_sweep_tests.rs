//! **C-SWEEP** (spec ch. 09, *Idle sweep*, v2.0.111): the guarantees held in
//! the shared crates, over the real document service, the regex hook
//! (`RegexDocumentIngest`) and encrypted in-memory stores.
//!
//! - **SW1:** the state follows the write's origin (Céline, 2026-11-17: «
//!   uniquement l'enregistrement via l'éditeur. Pas l'import de document. »):
//!   confirmed for a save or a creation through the editor, suspected
//!   (`Unreviewed`, with its token) for every other origin; the sweep makes one
//!   version tagged `pii-sweep`, marks the document analysed and leaves an
//!   analysed one alone; a pass is bounded; a state is never lowered by another
//!   device's ingest.
//! - **SW2:** the token's record id derived from the finding's identity; two
//!   devices that sweep the same version write the same version.
//! - **SW4:** the queue and the count at the last login are the persona's.
//! - **SW5:** review grouped by type; confirm and dismiss one or several;
//!   dismissing restores the value as written, one `pii-review` version; the
//!   dismissal holds through an edit of another paragraph; both gestures taken
//!   back; a label has one form, the one the model reads (the Fable maille,
//!   by delegation, 2026-11-17).
//! - **SW6:** the words of the signal and the count since the last login.
//! - **SW8:** the sweep detects by regex only.
//!
//! What a host shows, the walks, sync between devices and the phone's share
//! are held in the hosts' and sovereign-p2p's tests.
//!
//! Written before the code by the test author of lot LS(a); whoever implements
//! does not edit this file. A test that looks wrong is reported.

use std::sync::Arc;

use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{PiiKind, PiiRecord, ReviewState, SourceKind};
use sovereign_db::traits::GraphDB;
use sovereign_docformat::{BlockId, DocumentV2};
use sovereign_docs::{DocumentService, Edit, EditContent, NewContent, Origin, Provenance, TokenStatus, Trust};

use crate::docs_host::testing::{account_key, encrypted_memory_store};
use crate::docs_host::{build_document_service, create, new_document, new_edit_session, new_user_document};
use crate::pii::review::{state_words, PiiReview};
use crate::pii::signal;
use crate::pii::sweep::run_sweep_pass;

/// A Swiss IBAN the regex stage finds (`pii/regex.rs`, its own test).
const IBAN: &str = "CH93 0076 2011 6238 5295 7";
/// Another one, for a second value of the same type.
const IBAN2: &str = "CH56 0483 5012 3456 7800 9";

// ------------------------------------------------------------------ worlds --

/// One device's persona: its store, its service with the hook (account key
/// installed), and the same store's service without the key (ingest cannot
/// run: what it writes stays not analysed, C-DOC G10).
struct Device {
    store: Arc<EncryptedGraphDB>,
    docs: Arc<DocumentService>,
    unkeyed: Arc<DocumentService>,
    key: Arc<AccountKey>,
}

async fn device(tag: &str, name: &str, key: Arc<AccountKey>) -> Device {
    let store = encrypted_memory_store(tag).await;
    let docs = Arc::new(build_document_service(store.clone(), Some(key.clone()), name.into()));
    let unkeyed = Arc::new(build_document_service(store.clone(), None, name.into()));
    Device { store, docs, unkeyed, key }
}

/// A device of the user (her account key, shared by her paired devices).
async fn her_device(tag: &str, name: &str) -> Device {
    device(tag, name, account_key()).await
}

/// Another persona (a duress persona derives its own account key, ch. 07).
fn other_key() -> Arc<AccountKey> {
    let mk = MasterKey::from_passphrase(b"c-sweep-other-persona", b"salt").unwrap();
    Arc::new(AccountKey::derive(&mk).unwrap())
}

/// The discovered (not vault) records whose sources name `doc`.
async fn records_of(store: &EncryptedGraphDB, doc: &str) -> Vec<PiiRecord> {
    store
        .list_pii_records(None, None, Some(false))
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.deleted_at.is_none())
        .filter(|r| r.sources.iter().any(|s| s.source_kind == SourceKind::Document && s.source_id == doc))
        .collect()
}

/// The IBAN records of `doc`.
async fn ibans_of(store: &EncryptedGraphDB, doc: &str) -> Vec<PiiRecord> {
    records_of(store, doc).await.into_iter().filter(|r| r.kind == PiiKind::Iban).collect()
}

/// The one IBAN record of `doc`, with its state.
async fn the_iban(store: &EncryptedGraphDB, doc: &str) -> PiiRecord {
    let r = ibans_of(store, doc).await;
    assert_eq!(r.len(), 1, "one IBAN record for the document, found {}", r.len());
    r.into_iter().next().unwrap()
}

/// The document's text holds a token where the value was, never the value,
/// and that token resolves (C-DOC G15) to the record `record`.
async fn holds_token_not_value(docs: &DocumentService, doc: &str, value: &str, record: &PiiRecord) {
    let v = docs.read(doc).await.unwrap();
    assert!(!v.plain_text().contains(value), "the value was replaced by its token");
    let rid = record.id_string().unwrap();
    assert!(
        v.tokens.iter().any(|t| t.record_id == rid && matches!(t.status, TokenStatus::Token { .. })),
        "the document holds the record's token, resolving: {:?}",
        v.tokens
    );
}

fn editor() -> Origin {
    Origin::Editor { session: new_edit_session() }
}

fn edit(base: sovereign_docs::Revision, origin: Origin, content: EditContent) -> Edit {
    Edit { base, title: None, content: Some(content), origin, explicit: false, pasted_tokens: vec![] }
}

/// A document created without ingest (not analysed): `text` stored as given.
async fn unanalysed(d: &Device, title: &str, text: &str) -> String {
    let out = create(&d.unkeyed, new_user_document(title, "thread:t", text)).await.unwrap();
    assert!(!d.docs.read(&out.id).await.unwrap().pii_scanned, "test setup: created not analysed");
    out.id
}

/// Two text blocks: `first`, `second`. Returns the stored v2 form and the
/// second block's id.
fn two_paragraphs(first: &str, second: &str) -> (String, BlockId) {
    let mut doc = DocumentV2::from_text(first, "c-sweep-two");
    let b2 = BlockId::derived("c-sweep-two-second", 1);
    doc.insert_text_block(1, b2, second).unwrap();
    (doc.to_json_string(), b2)
}

async fn message_of_last_version(docs: &DocumentService, id: &str) -> Vec<String> {
    docs.versions(id).await.unwrap().into_iter().map(|v| v.message).collect()
}

// ======================================================================== SW1

/// SW1: a creation through the editor is where she sees the value replaced:
/// confirmed, with its token.
#[tokio::test]
async fn sw1_an_editor_creation_is_confirmed() {
    let d = her_device("sw1-editor-create", "a").await;
    let id = create(&d.docs, new_user_document("Bank", "thread:t", &format!("pay to {IBAN}"))).await.unwrap().id;
    let r = the_iban(&d.store, &id).await;
    assert_eq!(r.review_state, ReviewState::Confirmed);
    holds_token_not_value(&d.docs, &id, IBAN, &r).await;
}

/// SW1: a save through the editor: confirmed.
#[tokio::test]
async fn sw1_an_editor_save_is_confirmed() {
    let d = her_device("sw1-editor-save", "a").await;
    let id = create(&d.docs, new_user_document("Bank", "thread:t", "nothing yet")).await.unwrap().id;
    let base = d.docs.read(&id).await.unwrap().revision;
    d.docs.save(&id, edit(base, editor(), EditContent::PlainText(format!("pay to {IBAN}")))).await.unwrap();
    let r = the_iban(&d.store, &id).await;
    assert_eq!(r.review_state, ReviewState::Confirmed);
    holds_token_not_value(&d.docs, &id, IBAN, &r).await;
}

/// SW1 (v2.0.111): a save by the assistant shares the editor's write path,
/// not its origin: suspected, with its token.
#[tokio::test]
async fn sw1_an_assistant_save_is_suspected() {
    let d = her_device("sw1-assistant-save", "a").await;
    let id = create(&d.docs, new_user_document("Bank", "thread:t", "nothing yet")).await.unwrap().id;
    let base = d.docs.read(&id).await.unwrap().revision;
    d.docs.save(&id, edit(base, Origin::Assistant, EditContent::PlainText(format!("pay to {IBAN}")))).await.unwrap();
    let r = the_iban(&d.store, &id).await;
    assert_eq!(r.review_state, ReviewState::Unreviewed, "the assistant's write is not her editor");
    holds_token_not_value(&d.docs, &id, IBAN, &r).await;
}

/// SW1 (v2.0.111): a creation by the assistant: suspected.
#[tokio::test]
async fn sw1_an_assistant_creation_is_suspected() {
    let d = her_device("sw1-assistant-create", "a").await;
    let new = new_document("Bank", "thread:t", format!("pay to {IBAN}"), Trust::Owned, Provenance::Written, Origin::Assistant);
    let id = create(&d.docs, new).await.unwrap().id;
    let r = the_iban(&d.store, &id).await;
    assert_eq!(r.review_state, ReviewState::Unreviewed);
    holds_token_not_value(&d.docs, &id, IBAN, &r).await;
}

/// SW1: an import (« Pas l'import de document »): suspected.
#[tokio::test]
async fn sw1_an_import_is_suspected() {
    let d = her_device("sw1-import", "a").await;
    let new = new_document(
        "statement.txt",
        "thread:t",
        format!("pay to {IBAN}"),
        Trust::Undetermined,
        Provenance::Import { source_path: "C:/tmp/statement.txt".into() },
        Origin::Import,
    );
    let id = create(&d.docs, new).await.unwrap().id;
    let r = the_iban(&d.store, &id).await;
    assert_eq!(r.review_state, ReviewState::Unreviewed);
    holds_token_not_value(&d.docs, &id, IBAN, &r).await;
}

/// SW1: a skill's output: suspected.
#[tokio::test]
async fn sw1_a_skill_output_is_suspected() {
    let d = her_device("sw1-skill", "a").await;
    let id = create(&d.docs, new_user_document("Bank", "thread:t", "nothing yet")).await.unwrap().id;
    let base = d.docs.read(&id).await.unwrap().revision;
    d.docs.apply_skill_output(&id, base, "formatter", EditContent::PlainText(format!("pay to {IBAN}"))).await.unwrap();
    let r = the_iban(&d.store, &id).await;
    assert_eq!(r.review_state, ReviewState::Unreviewed);
    holds_token_not_value(&d.docs, &id, IBAN, &r).await;
}

/// SW1: the restore of a version holding a value never analysed: suspected.
#[tokio::test]
async fn sw1_a_restored_version_is_suspected() {
    let d = her_device("sw1-restore", "a").await;
    let id = unanalysed(&d, "Bank", &format!("pay to {IBAN}")).await;
    let base = d.unkeyed.read(&id).await.unwrap().revision;
    d.unkeyed.save(&id, edit(base, editor(), EditContent::PlainText("rewritten".into()))).await.unwrap();
    let mut with_iban = None;
    for v in d.docs.versions(&id).await.unwrap() {
        let view = d.docs.read_version(&id, &v.commit_id).await.unwrap();
        if view.doc.map(|doc| doc.plain_text().contains(IBAN)).unwrap_or(false) {
            with_iban = Some(v.commit_id);
        }
    }
    let commit = with_iban.expect("test setup: a version holds the raw IBAN");
    d.docs.restore_version(&id, &commit).await.unwrap();
    let r = the_iban(&d.store, &id).await;
    assert_eq!(r.review_state, ReviewState::Unreviewed);
    holds_token_not_value(&d.docs, &id, IBAN, &r).await;
}

/// SW1: content received by sync that no device analysed: suspected, with its
/// token, on the device that receives it.
#[tokio::test]
async fn sw1_content_received_unanalysed_is_suspected() {
    let a = her_device("sw1-sync-a", "a").await;
    let b = her_device("sw1-sync-b", "b").await;
    let id = unanalysed(&a, "Bank", &format!("pay to {IBAN}")).await;
    let st = a.docs.stored_state(&id).await.unwrap();
    b.docs.apply_wire(st.document, st.meta).await.unwrap();
    let r = the_iban(&b.store, &id).await;
    assert_eq!(r.review_state, ReviewState::Unreviewed);
    holds_token_not_value(&b.docs, &id, IBAN, &r).await;
}

/// SW1 (v2.0.112, Céline, 2026-11-17): a page saved from the browser is not
/// « through the editor »; it takes the import's origin: suspected.
#[tokio::test]
async fn sw1_a_page_saved_from_the_browser_is_suspected() {
    let d = her_device("sw1-web", "a").await;
    let new = crate::docs_host::new_web_document("Bank page", "thread:t", "https://bank.example/iban", format!("pay to {IBAN}"));
    assert_eq!(new.origin, Origin::Import, "a saved page takes the import's origin");
    let id = create(&d.docs, new).await.unwrap().id;
    let r = the_iban(&d.store, &id).await;
    assert_eq!(r.review_state, ReviewState::Unreviewed);
    holds_token_not_value(&d.docs, &id, IBAN, &r).await;
}

/// SW1: a state is never lowered by another device's ingest. The confirmed
/// record arrives first (inserted under its id, as sync inserts a record),
/// then its document: the record keeps its state, and the receiving hook
/// writes no second record for the value.
#[tokio::test]
async fn sw1_a_confirmed_record_received_first_keeps_its_state() {
    let a = her_device("sw1-order-a", "a").await;
    let b = her_device("sw1-order-b", "b").await;
    let id = create(&a.docs, new_user_document("Bank", "thread:t", &format!("pay to {IBAN}"))).await.unwrap().id;
    let rec = the_iban(&a.store, &id).await;
    assert_eq!(rec.review_state, ReviewState::Confirmed, "test setup: confirmed on the device that saw it");
    assert!(b.store.create_pii_record_with_id(rec.clone()).await.unwrap());
    let st = a.docs.stored_state(&id).await.unwrap();
    b.docs.apply_wire(st.document, st.meta).await.unwrap();
    let on_b = ibans_of(&b.store, &id).await;
    assert_eq!(on_b.len(), 1, "no second record for the value");
    assert_eq!(on_b[0].id_string(), rec.id_string());
    assert_eq!(on_b[0].review_state, ReviewState::Confirmed, "never lowered by another device's ingest");
}

// ---------------------------------------------------------------- the sweep --

/// SW1: the sweep of a document not yet analysed: each value replaced by its
/// token, suspected, in the body and in the title; one new version, tagged
/// for the sweep (« Personal-data sweep »); the document marked analysed.
#[tokio::test]
async fn sw1_the_sweep_replaces_suspects_versions_once_and_marks() {
    let d = her_device("sw1-sweep", "a").await;
    let id = create(&d.unkeyed, new_user_document(format!("IBAN {IBAN2}"), "thread:t", format!("pay to {IBAN}")))
        .await
        .unwrap()
        .id;
    let before = d.docs.versions(&id).await.unwrap().len();

    let out = d.docs.sweep(&id).await.unwrap();
    assert!(out.changed);

    let view = d.docs.read(&id).await.unwrap();
    assert!(view.pii_scanned, "marked analysed (pii_scanned_at, as built)");
    assert!(!view.plain_text().contains(IBAN), "the body's value replaced");
    assert!(!view.title.contains(IBAN2), "the title's value replaced");
    let records = ibans_of(&d.store, &id).await;
    assert_eq!(records.len(), 2);
    for r in &records {
        assert_eq!(r.review_state, ReviewState::Unreviewed, "what the sweep replaced is suspected");
    }
    let messages = message_of_last_version(&d.docs, &id).await;
    assert_eq!(messages.len(), before + 1, "one new version: {messages:?}");
    assert!(messages.iter().any(|m| m == "Personal-data sweep"), "the version says who wrote it: {messages:?}");
}

/// SW1: an analysed document is never touched by the sweep: no change, no
/// version, its records as they were.
#[tokio::test]
async fn sw1_the_sweep_leaves_an_analysed_document_alone() {
    let d = her_device("sw1-sweep-analysed", "a").await;
    let id = create(&d.docs, new_user_document("Bank", "thread:t", &format!("pay to {IBAN}"))).await.unwrap().id;
    let versions = d.docs.versions(&id).await.unwrap().len();
    let text = d.docs.read(&id).await.unwrap().plain_text();
    let out = d.docs.sweep(&id).await.unwrap();
    assert!(!out.changed);
    assert_eq!(d.docs.versions(&id).await.unwrap().len(), versions);
    assert_eq!(d.docs.read(&id).await.unwrap().plain_text(), text);
    assert_eq!(the_iban(&d.store, &id).await.review_state, ReviewState::Confirmed);
}

/// SW1: a pass is bounded; the next pass takes what is left; an analysed
/// document is never in a pass.
#[tokio::test]
async fn sw1_a_pass_is_bounded_and_the_next_takes_the_rest() {
    let d = her_device("sw1-batches", "a").await;
    let analysed = create(&d.docs, new_user_document("Done", "thread:t", "already analysed")).await.unwrap().id;
    let mut todo = vec![];
    for i in 0..3 {
        todo.push(unanalysed(&d, &format!("Pile {i}"), &format!("text {i} {IBAN}")).await);
    }
    assert_eq!(d.docs.unanalysed(10).await.unwrap().len(), 3);

    let first = run_sweep_pass(&d.docs, 2).await;
    assert_eq!(first.analysed.len(), 2, "{first:?}");
    assert!(first.remaining);
    let second = run_sweep_pass(&d.docs, 2).await;
    assert_eq!(second.analysed.len(), 1, "{second:?}");
    assert!(!second.remaining);
    let third = run_sweep_pass(&d.docs, 2).await;
    assert!(third.analysed.is_empty());

    let mut swept: Vec<String> = first.analysed.into_iter().chain(second.analysed).collect();
    swept.sort();
    todo.sort();
    assert_eq!(swept, todo, "each document not analysed, once; never the analysed one ({analysed})");
    for id in &todo {
        assert!(d.docs.read(id).await.unwrap().pii_scanned);
    }
}

// ======================================================================== SW2

/// SW2: the record id is derived from the finding's identity: the same
/// inputs give the same id; each input that differs gives another.
#[test]
fn sw2_the_record_id_is_derived_from_the_finding() {
    use crate::pii::finding_id::finding_record_id;
    use sovereign_docs::TextField;
    let b = BlockId::derived("sw2", 0);
    let base = finding_record_id("document:abc", "v1", &TextField::Text(b), 3, 29);
    assert!(base.starts_with("pii_record:"), "{base}");
    assert!(sovereign_db::schema::raw_to_thing(&base).is_some(), "a valid record id of the store: {base}");
    assert_eq!(base, finding_record_id("document:abc", "v1", &TextField::Text(b), 3, 29));
    let others = [
        finding_record_id("document:abd", "v1", &TextField::Text(b), 3, 29),
        finding_record_id("document:abc", "v2", &TextField::Text(b), 3, 29),
        finding_record_id("document:abc", "v1", &TextField::Title, 3, 29),
        finding_record_id("document:abc", "v1", &TextField::Caption(b), 3, 29),
        finding_record_id("document:abc", "v1", &TextField::Text(b), 4, 29),
        finding_record_id("document:abc", "v1", &TextField::Text(b), 3, 30),
    ];
    for o in &others {
        assert_ne!(&base, o);
    }
}

/// SW2: two devices that sweep the same version write the same new version:
/// the same text, the same record ids, the same normalised hash, so it
/// converges by its hash instead of making two concurrent versions.
#[tokio::test]
async fn sw2_two_devices_sweeping_one_version_write_the_same_version() {
    let a = her_device("sw2-a", "a").await;
    let b = her_device("sw2-b", "b").await;
    let id = unanalysed(&a, &format!("IBAN {IBAN2}"), &format!("pay to {IBAN}")).await;
    let st = a.unkeyed.stored_state(&id).await.unwrap();
    b.unkeyed.apply_wire(st.document, st.meta).await.unwrap();
    assert!(!b.docs.read(&id).await.unwrap().pii_scanned, "test setup: not analysed on either device");

    assert!(a.docs.sweep(&id).await.unwrap().changed);
    assert!(b.docs.sweep(&id).await.unwrap().changed);

    let (va, vb) = (a.docs.read(&id).await.unwrap(), b.docs.read(&id).await.unwrap());
    assert_eq!(va.title, vb.title);
    assert_eq!(va.plain_text(), vb.plain_text());
    let ids = |v: &sovereign_docs::DocumentView| {
        let mut r: Vec<String> = v.tokens.iter().map(|t| t.record_id.clone()).collect();
        r.sort();
        r
    };
    assert_eq!(ids(&va).len(), 2);
    assert_eq!(ids(&va), ids(&vb), "the same record ids on both devices");
    assert_eq!(
        a.docs.stored_state(&id).await.unwrap().normalized_hash,
        b.docs.stored_state(&id).await.unwrap().normalized_hash
    );
}

// ======================================================================== SW4

/// SW4: the queue, and the count seen at the last login, are the persona's:
/// another persona (its own store and account key, as the duress persona) sees
/// none of them, and its logins move nothing of the first's.
#[tokio::test]
async fn sw4_another_persona_never_sees_the_queue() {
    let primary = her_device("sw4-primary", "a").await;
    let duress = device("sw4-duress", "a", other_key()).await;
    unanalysed(&primary, "Bank", &format!("pay to {IBAN}")).await;
    run_sweep_pass(&primary.docs, 10).await;
    assert_eq!(signal::waiting(primary.store.as_ref()).await.unwrap(), 1);

    let review = PiiReview::new(duress.docs.clone(), duress.key.clone());
    assert!(review.groups().await.unwrap().iter().all(|g| g.rows.is_empty()), "nothing of the primary's queue");
    assert_eq!(signal::waiting(duress.store.as_ref()).await.unwrap(), 0);
    assert_eq!(signal::login_signal(duress.store.as_ref()).await.unwrap().more_since_last_login, 0);

    let s = signal::login_signal(primary.store.as_ref()).await.unwrap();
    assert_eq!((s.waiting, s.more_since_last_login), (1, 1), "the duress login moved nothing of the primary's count");
}

// ======================================================================== SW5

/// SW5: the window groups every record by type, whatever its state, across
/// documents; each row says its state in words (the Fable maille, by
/// delegation of Céline, 2026-11-17).
#[tokio::test]
async fn sw5_the_window_groups_by_type_with_the_state_in_words() {
    let d = her_device("sw5-groups", "a").await;
    let a = create(&d.docs, new_user_document("A", "thread:t", &format!("{IBAN} and alice@example.ch"))).await.unwrap().id;
    let b = unanalysed(&d, "B", &format!("pay to {IBAN2}")).await;
    assert!(d.docs.sweep(&b).await.unwrap().changed);

    let review = PiiReview::new(d.docs.clone(), d.key.clone());
    let groups = review.groups().await.unwrap();
    let iban = groups.iter().find(|g| g.kind == PiiKind::Iban).expect("an IBAN group");
    let email = groups.iter().find(|g| g.kind == PiiKind::Email).expect("an e-mail group");
    assert_eq!(iban.rows.len(), 2, "the IBANs of both documents in one group");
    assert_eq!(email.rows.len(), 1);
    for g in &groups {
        assert!(g.rows.iter().all(|r| r.state_words == state_words(&r.state)));
    }
    let row_of = |doc: &str| iban.rows.iter().find(|r| r.documents.iter().any(|x| x == doc)).unwrap().clone();
    assert_eq!(row_of(&a).state_words, "PII confirmed");
    assert_eq!(row_of(&b).state_words, "PII suspected - TBC");
}

/// SW5, the words of each state: her own label (Céline, 2026-11-17: « je
/// préfère mon libellé, Suspected seul ne dit pas ce qui est suspected »),
/// replacing the Fable maille's first wording of the same day.
#[test]
fn sw5_the_states_in_words() {
    assert_eq!(state_words(&ReviewState::Unreviewed), "PII suspected - TBC");
    assert_eq!(state_words(&ReviewState::Confirmed), "PII confirmed");
    assert_eq!(state_words(&ReviewState::Dismissed), "PII dismissed");
}

/// SW5: confirming a suspected value keeps its token and makes it confirmed;
/// no version (the text does not change).
#[tokio::test]
async fn sw5_confirming_a_suspected_value_keeps_its_token() {
    let d = her_device("sw5-confirm", "a").await;
    let id = unanalysed(&d, "Bank", &format!("pay to {IBAN}")).await;
    d.docs.sweep(&id).await.unwrap();
    let r = the_iban(&d.store, &id).await;
    let text = d.docs.read(&id).await.unwrap().plain_text();
    let versions = d.docs.versions(&id).await.unwrap().len();

    PiiReview::new(d.docs.clone(), d.key.clone()).confirm(&[r.id_string().unwrap()]).await.unwrap();
    let after = the_iban(&d.store, &id).await;
    assert_eq!(after.review_state, ReviewState::Confirmed);
    assert_eq!(d.docs.read(&id).await.unwrap().plain_text(), text, "the token stays");
    assert_eq!(d.docs.versions(&id).await.unwrap().len(), versions);
    assert_eq!(signal::waiting(d.store.as_ref()).await.unwrap(), 0);
}

/// SW5: any record can be dismissed, a save-confirmed one too: the value is
/// restored as written, one new version tagged for the review
/// (« Personal-data review »), the record dismissed.
#[tokio::test]
async fn sw5_dismissing_a_saved_value_restores_it_in_one_version() {
    let d = her_device("sw5-dismiss", "a").await;
    let id = create(&d.docs, new_user_document(format!("IBAN {IBAN}"), "thread:t", format!("pay to {IBAN}")))
        .await
        .unwrap()
        .id;
    let records = ibans_of(&d.store, &id).await;
    assert!(!records.is_empty());
    let before = d.docs.versions(&id).await.unwrap().len();

    let ids: Vec<String> = records.iter().map(|r| r.id_string().unwrap()).collect();
    let out = PiiReview::new(d.docs.clone(), d.key.clone()).dismiss(&ids).await.unwrap();
    assert_eq!(out.rewritten, vec![id.clone()]);

    let view = d.docs.read(&id).await.unwrap();
    assert!(view.plain_text().contains(IBAN), "the body as written");
    assert!(view.title.contains(IBAN), "the title as written");
    let messages = message_of_last_version(&d.docs, &id).await;
    assert_eq!(messages.len(), before + 1, "one new version: {messages:?}");
    assert!(messages.iter().any(|m| m == "Personal-data review"), "{messages:?}");
    for r in ibans_of(&d.store, &id).await {
        assert_eq!(r.review_state, ReviewState::Dismissed);
    }
}

/// SW5: several records selected at once, in two documents: one new version
/// per document.
#[tokio::test]
async fn sw5_dismissing_several_makes_one_version_per_document() {
    let d = her_device("sw5-several", "a").await;
    let a = create(&d.docs, new_user_document("A", "thread:t", &format!("{IBAN} then {IBAN2}"))).await.unwrap().id;
    let b = create(&d.docs, new_user_document("B", "thread:t", &format!("pay to {IBAN}"))).await.unwrap().id;
    let (va, vb) = (d.docs.versions(&a).await.unwrap().len(), d.docs.versions(&b).await.unwrap().len());
    let mut ids: Vec<String> = ibans_of(&d.store, &a).await.iter().map(|r| r.id_string().unwrap()).collect();
    ids.extend(ibans_of(&d.store, &b).await.iter().map(|r| r.id_string().unwrap()));
    assert_eq!(ids.len(), 3);

    let mut out = PiiReview::new(d.docs.clone(), d.key.clone()).dismiss(&ids).await.unwrap();
    out.rewritten.sort();
    let mut want = vec![a.clone(), b.clone()];
    want.sort();
    assert_eq!(out.rewritten, want);
    assert_eq!(d.docs.versions(&a).await.unwrap().len(), va + 1, "one version for the two values of A");
    assert_eq!(d.docs.versions(&b).await.unwrap().len(), vb + 1);
}

/// SW5, the test of the contract (here; the second device is held in
/// sovereign-p2p's tests): she dismisses a value, edits another paragraph and
/// saves. The value is not back in the queue, not replaced again, and no new
/// record is written for it.
#[tokio::test]
async fn sw5_a_dismissal_holds_when_another_paragraph_is_saved() {
    let d = her_device("sw5-holds", "a").await;
    let (v2, second) = two_paragraphs(&format!("pay to {IBAN}"), "a note");
    let mut new = new_user_document("Bank", "thread:t", "");
    new.content = NewContent::V2(v2);
    let id = create(&d.docs, new).await.unwrap().id;
    let r = the_iban(&d.store, &id).await;
    PiiReview::new(d.docs.clone(), d.key.clone()).dismiss(&[r.id_string().unwrap()]).await.unwrap();

    let base = d.docs.read(&id).await.unwrap().revision;
    d.docs
        .save(&id, edit(base, editor(), EditContent::TextBlocks(vec![(second, "a longer note".into())])))
        .await
        .unwrap();

    let view = d.docs.read(&id).await.unwrap();
    assert!(view.plain_text().contains(IBAN), "the dismissed value stands as written");
    assert!(view.plain_text().contains("a longer note"));
    let records = ibans_of(&d.store, &id).await;
    assert_eq!(records.len(), 1, "no new record for a dismissed value");
    assert_eq!(records[0].review_state, ReviewState::Dismissed);
    assert_eq!(signal::waiting(d.store.as_ref()).await.unwrap(), 0, "not back in the queue");
}

/// SW5: taking back a dismissal puts the token back, confirmed (she has now
/// seen it), one new version.
#[tokio::test]
async fn sw5_taking_back_a_dismissal_tokenises_again_confirmed() {
    let d = her_device("sw5-undo-dismiss", "a").await;
    let id = unanalysed(&d, "Bank", &format!("pay to {IBAN}")).await;
    d.docs.sweep(&id).await.unwrap();
    let rid = the_iban(&d.store, &id).await.id_string().unwrap();
    let review = PiiReview::new(d.docs.clone(), d.key.clone());
    review.dismiss(&[rid.clone()]).await.unwrap();
    let versions = d.docs.versions(&id).await.unwrap().len();

    let out = review.take_back_dismissal(&[rid]).await.unwrap();
    assert_eq!(out.rewritten, vec![id.clone()]);
    let r = the_iban(&d.store, &id).await;
    assert_eq!(r.review_state, ReviewState::Confirmed);
    holds_token_not_value(&d.docs, &id, IBAN, &r).await;
    assert_eq!(d.docs.versions(&id).await.unwrap().len(), versions + 1);
}

/// SW5: taking back the confirmation of a suspected value makes it suspected
/// again; its token stays, no version.
#[tokio::test]
async fn sw5_taking_back_a_confirmation_suspects_again_and_keeps_the_token() {
    let d = her_device("sw5-undo-confirm", "a").await;
    let id = unanalysed(&d, "Bank", &format!("pay to {IBAN}")).await;
    d.docs.sweep(&id).await.unwrap();
    let rid = the_iban(&d.store, &id).await.id_string().unwrap();
    let review = PiiReview::new(d.docs.clone(), d.key.clone());
    review.confirm(&[rid.clone()]).await.unwrap();
    let versions = d.docs.versions(&id).await.unwrap().len();

    review.take_back_confirmation(&[rid]).await.unwrap();
    let r = the_iban(&d.store, &id).await;
    assert_eq!(r.review_state, ReviewState::Unreviewed);
    holds_token_not_value(&d.docs, &id, IBAN, &r).await;
    assert_eq!(d.docs.versions(&id).await.unwrap().len(), versions);
    assert_eq!(signal::waiting(d.store.as_ref()).await.unwrap(), 1);
}

// ======================================================================== SW6

/// SW6: the status bar's words (the Fable maille, by delegation of Céline,
/// 2026-11-17); nothing when none waits.
#[test]
fn sw6_the_status_words() {
    assert_eq!(signal::status_words(0), None);
    assert_eq!(signal::status_words(1).as_deref(), Some("1 personal data to review"));
    assert_eq!(signal::status_words(12).as_deref(), Some("12 personal data to review"));
}

/// SW6: the notice at login says how many more since the last login, and how
/// many in all; none when the queue has not grown.
#[test]
fn sw6_the_login_notice_words() {
    use signal::{login_notice, LoginSignal};
    assert_eq!(login_notice(&LoginSignal { waiting: 5, more_since_last_login: 0 }), None);
    assert_eq!(
        login_notice(&LoginSignal { waiting: 12, more_since_last_login: 3 }),
        Some((
            "Personal data to review".to_string(),
            "3 more since your last login, 12 in all. They wait in the PII dashboard.".to_string()
        ))
    );
}

/// SW6: the count at the last login is renewed at each login, whether the
/// notice showed or not; only suspected values count.
#[tokio::test]
async fn sw6_the_count_since_the_last_login() {
    let d = her_device("sw6-count", "a").await;
    create(&d.docs, new_user_document("Mine", "thread:t", &format!("pay to {IBAN}"))).await.unwrap();
    assert_eq!(signal::waiting(d.store.as_ref()).await.unwrap(), 0, "a value she saw replaced does not wait");

    unanalysed(&d, "One", &format!("pay to {IBAN}")).await;
    unanalysed(&d, "Two", &format!("pay to {IBAN2}")).await;
    run_sweep_pass(&d.docs, 10).await;
    let first = signal::login_signal(d.store.as_ref()).await.unwrap();
    assert_eq!((first.waiting, first.more_since_last_login), (2, 2));
    let again = signal::login_signal(d.store.as_ref()).await.unwrap();
    assert_eq!((again.waiting, again.more_since_last_login), (2, 0), "renewed at the previous login");

    unanalysed(&d, "Three", &format!("again {IBAN}")).await;
    run_sweep_pass(&d.docs, 10).await;
    let third = signal::login_signal(d.store.as_ref()).await.unwrap();
    assert_eq!((third.waiting, third.more_since_last_login), (3, 1));
}

// ======================================================================== SW8

/// SW8: the sweep detects by regex only: a person's name stays as written, an
/// IBAN beside it is replaced.
#[tokio::test]
async fn sw8_the_sweep_detects_by_regex_only() {
    let d = her_device("sw8-regex", "a").await;
    let id = unanalysed(&d, "Meeting", &format!("Met Charlie Newcomer, who gave {IBAN}.")).await;
    assert!(d.docs.sweep(&id).await.unwrap().changed);
    let text = d.docs.read(&id).await.unwrap().plain_text();
    assert!(text.contains("Charlie Newcomer"), "no named entity is detected");
    assert!(!text.contains(IBAN));
    assert!(records_of(&d.store, &id).await.iter().all(|r| r.kind != PiiKind::PersonName));
}

/// SW5 (v2.0.112): taking back a confirmation makes the value suspected again
/// whoever confirmed it, a save in the editor included; its token stays, no
/// version.
#[tokio::test]
async fn sw5_taking_back_an_editor_confirmation_suspects_it() {
    let d = her_device("sw5-undo-editor", "a").await;
    let id = create(&d.docs, new_user_document("Bank", "thread:t", &format!("pay to {IBAN}"))).await.unwrap().id;
    let r = the_iban(&d.store, &id).await;
    assert_eq!(r.review_state, ReviewState::Confirmed, "test setup: confirmed by the editor's save");
    let versions = d.docs.versions(&id).await.unwrap().len();

    PiiReview::new(d.docs.clone(), d.key.clone()).take_back_confirmation(&[r.id_string().unwrap()]).await.unwrap();
    let after = the_iban(&d.store, &id).await;
    assert_eq!(after.review_state, ReviewState::Unreviewed);
    holds_token_not_value(&d.docs, &id, IBAN, &after).await;
    assert_eq!(d.docs.versions(&id).await.unwrap().len(), versions);
}

/// SW1 (« Only her review changes a suspected value's state »; added on the
/// Fable maille's read of 2026-11-17): she takes back a confirmation, then
/// types the same value again in that document and saves in the editor. The
/// value stays suspected: a save confirms what it replaces, not a value she
/// has put back in the queue.
#[tokio::test]
async fn sw1_a_value_taken_back_stays_suspected_when_typed_again_in_the_editor() {
    let d = her_device("sw1-typed-again", "a").await;
    let id = create(&d.docs, new_user_document("Bank", "thread:t", &format!("pay to {IBAN}"))).await.unwrap().id;
    let r = the_iban(&d.store, &id).await;
    PiiReview::new(d.docs.clone(), d.key.clone()).take_back_confirmation(&[r.id_string().unwrap()]).await.unwrap();

    let view = d.docs.read(&id).await.unwrap();
    let text = format!("{} and again {IBAN}", view.plain_text());
    d.docs.save(&id, edit(view.revision, editor(), EditContent::PlainText(text))).await.unwrap();

    let records = ibans_of(&d.store, &id).await;
    assert!(!records.is_empty());
    for rec in &records {
        assert_eq!(rec.review_state, ReviewState::Unreviewed, "the value stays suspected: only her review changes it");
    }
    assert!(!d.docs.read(&id).await.unwrap().plain_text().contains(IBAN), "replaced by its token, as a save does");
}

// ------------------------------------------------------------------ copies --
// The Fable maille, by delegation of Céline, 2026-11-17: in a copy, (1) a token
// keeps the same record and state, and reviewing that record settles it in
// every document linked to it; (2) a value dismissed in the source is dismissed
// in the copy too, then independent; (3) a value the source had not analysed
// is replaced by the copy's ingest as suspected, whoever asked for the copy.

/// Copies (1): a token keeps its record and state; a dismissal of that record
/// restores the value in the source and in the copy, one version each.
#[tokio::test]
async fn sw5_a_copys_token_keeps_its_record_and_a_review_settles_both() {
    let d = her_device("copy-token", "a").await;
    let src = create(&d.docs, new_user_document("Bank", "thread:t", &format!("pay to {IBAN}"))).await.unwrap().id;
    let r = the_iban(&d.store, &src).await;
    let copy = crate::docs_host::duplicate(&d.docs, &src, "Copy".into(), "thread:t".into(), editor()).await.unwrap().id;
    holds_token_not_value(&d.docs, &copy, IBAN, &r).await;
    assert_eq!(the_iban(&d.store, &copy).await.id_string(), r.id_string(), "the same record");
    let (vs, vc) = (d.docs.versions(&src).await.unwrap().len(), d.docs.versions(&copy).await.unwrap().len());

    let mut out = PiiReview::new(d.docs.clone(), d.key.clone()).dismiss(&[r.id_string().unwrap()]).await.unwrap();
    out.rewritten.sort();
    let mut want = vec![src.clone(), copy.clone()];
    want.sort();
    assert_eq!(out.rewritten, want);
    for (id, before) in [(&src, vs), (&copy, vc)] {
        assert!(d.docs.read(id).await.unwrap().plain_text().contains(IBAN), "restored as written in {id}");
        assert_eq!(d.docs.versions(id).await.unwrap().len(), before + 1);
    }
}

/// Copies (2): a value dismissed in the source is dismissed in the copy: it
/// stands as written there, through a save in the copy too, and waits nowhere.
#[tokio::test]
async fn sw5_a_value_dismissed_in_the_source_is_dismissed_in_the_copy() {
    let d = her_device("copy-dismissed", "a").await;
    let src = create(&d.docs, new_user_document("Bank", "thread:t", &format!("pay to {IBAN}"))).await.unwrap().id;
    let r = the_iban(&d.store, &src).await;
    PiiReview::new(d.docs.clone(), d.key.clone()).dismiss(&[r.id_string().unwrap()]).await.unwrap();

    let copy = crate::docs_host::duplicate(&d.docs, &src, "Copy".into(), "thread:t".into(), editor()).await.unwrap().id;
    assert!(d.docs.read(&copy).await.unwrap().plain_text().contains(IBAN), "the copy holds it as written");
    let base = d.docs.read(&copy).await.unwrap().revision;
    d.docs
        .save(&copy, Edit { base, title: Some("Copy, renamed".into()), content: None, origin: editor(), explicit: false, pasted_tokens: vec![] })
        .await
        .unwrap();
    assert!(d.docs.read(&copy).await.unwrap().plain_text().contains(IBAN), "still as written after a save in the copy");
    for rec in ibans_of(&d.store, &copy).await {
        assert_eq!(rec.review_state, ReviewState::Dismissed, "no live record for the copy's dismissed value");
    }
    assert_eq!(signal::waiting(d.store.as_ref()).await.unwrap(), 0);
}

/// Copies (3): a value the source had not analysed is replaced by the copy's
/// ingest as suspected, even when the copy is asked for from the editor.
#[tokio::test]
async fn sw1_a_copy_of_an_unanalysed_value_is_suspected_whoever_asked() {
    let d = her_device("copy-unanalysed", "a").await;
    let src = unanalysed(&d, "Bank", &format!("pay to {IBAN}")).await;
    let copy = crate::docs_host::duplicate(&d.docs, &src, "Copy".into(), "thread:t".into(), editor()).await.unwrap().id;
    let r = the_iban(&d.store, &copy).await;
    assert_eq!(r.review_state, ReviewState::Unreviewed, "the copy's ingest is never the editor's");
    holds_token_not_value(&d.docs, &copy, IBAN, &r).await;
}

/// Every type of personal data, for the label test below. The match has no
/// wildcard, so a new type cannot be added without being listed here.
fn every_kind() -> Vec<PiiKind> {
    let all = vec![
        PiiKind::Email,
        PiiKind::Phone,
        PiiKind::Ssn,
        PiiKind::CreditCard,
        PiiKind::Ipv4,
        PiiKind::Avs,
        PiiKind::Iban,
        PiiKind::Passport,
        PiiKind::Dob,
        PiiKind::Address,
        PiiKind::PersonName,
        PiiKind::OrgName,
        PiiKind::Password,
        PiiKind::ApiToken,
        PiiKind::BankAccount,
        PiiKind::DocumentId,
        PiiKind::Note,
        PiiKind::Other,
    ];
    for k in &all {
        match k {
            PiiKind::Email
            | PiiKind::Phone
            | PiiKind::Ssn
            | PiiKind::CreditCard
            | PiiKind::Ipv4
            | PiiKind::Avs
            | PiiKind::Iban
            | PiiKind::Passport
            | PiiKind::Dob
            | PiiKind::Address
            | PiiKind::PersonName
            | PiiKind::OrgName
            | PiiKind::Password
            | PiiKind::ApiToken
            | PiiKind::BankAccount
            | PiiKind::DocumentId
            | PiiKind::Note
            | PiiKind::Other => {}
        }
    }
    all
}

/// SW5, « A label has one form » (the Fable maille, by delegation of Céline,
/// 2026-11-17): the one the model reads (`[IBAN]`, `[Card]`, `[IP]`,
/// `[Name]`…), in titles and text alike; a type's code name (`[Iban]`) is
/// never shown.
///
/// The form the model reads is the table of `pii::resolve`
/// (`preview_label_for_kind`, what the masking at read puts in a prompt, SW3).
/// (1) A document created through the service with an IBAN in its title and
/// its text: its labelled title and labelled text show `[IBAN]`, never
/// `[Iban]`, never the value. (2) For every type, a token of a record of that
/// type, in a document that holds it, gets the table's label from the service
/// — so no type gives its code name where the two differ (`[{:?}]`).
#[tokio::test]
async fn sw5_a_label_has_one_form_the_one_the_model_reads() {
    use crate::pii::resolve::preview_label_for_kind;

    let d = her_device("one-label-form", "a").await;
    let id = create(&d.docs, new_user_document(format!("Transfer {IBAN}"), "thread:t", format!("pay to {IBAN} today")))
        .await
        .unwrap()
        .id;
    let v = d.docs.read(&id).await.unwrap();
    assert!(v.title.contains("[pii:"), "precondition: the title holds a token: {:?}", v.title);
    for (what, shown) in [("title", v.labelled_title()), ("text", v.labelled_text())] {
        assert!(shown.contains("[IBAN]"), "the {what} shows the table's form: {shown}");
        assert!(!shown.contains("[Iban]"), "the {what} never shows the type's code name: {shown}");
        assert!(!shown.contains(IBAN), "the {what} never shows the value: {shown}");
    }

    // Each type's record is the IBAN's, as the save wrote it (linked to this
    // document), with another type: a new record, nothing else changed.
    let base = the_iban(&d.store, &id).await;
    let mut wrong = vec![];
    for kind in every_kind() {
        let rec = d
            .store
            .create_pii_record(PiiRecord { id: None, kind: kind.clone(), ..base.clone() })
            .await
            .unwrap();
        let rid = rec.id_string().unwrap();
        let table = preview_label_for_kind(&kind);
        match d.docs.token_status(&id, &rid).await.unwrap() {
            TokenStatus::Token { label } if label == table => {}
            TokenStatus::Token { label } => {
                wrong.push(format!("{kind:?}: shown {label}, the table's form is {table} (code name [{kind:?}])"))
            }
            other => wrong.push(format!("{kind:?}: not a token here: {other:?}")),
        }
    }
    assert!(wrong.is_empty(), "a label has one form, the table's:\n{}", wrong.join("\n"));
}
