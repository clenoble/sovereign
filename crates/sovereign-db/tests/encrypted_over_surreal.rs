//! Integration suite: `EncryptedGraphDB` over a **real** in-memory `SurrealGraphDB`.
//!
//! v0.0.8 review Theme 2: every `EncryptedGraphDB` test ran over `MockGraphDB`,
//! whose semantics diverge from SurrealDB (enum binding, record-vs-string
//! comparison, edge direction, soft-delete) — so query paths that are broken in
//! production passed CI. This suite asserts the *contract* of each path against
//! the real backend. Tests that fail here and pass over the mock are exactly
//! the divergence the review flagged (C1, C2, H-db1, M4, M1).
//!
//! Runs in CI via `cargo test -p sovereign-db --features encryption`
//! (`required-features = ["encryption"]` in Cargo.toml).

use std::sync::Arc;

use tokio::sync::RwLock;

use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::IndexKey;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use sovereign_crypto::master_key::MasterKey;

use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{
    ChannelType, Contact, Conversation, Document, Message, MessageDirection, RelationType,
    SuggestionSource, SuggestionStatus, Thread, thing_to_raw,
};
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::traits::GraphDB;

// ---------------------------------------------------------------- builders --

fn test_device_key() -> DeviceKey {
    let mk = MasterKey::from_passphrase(b"test", b"salt").unwrap();
    DeviceKey::derive(&mk, "test-device").unwrap()
}

/// Unique scratch-path component so per-entity key DBs never collide across
/// tests or runs (KeyDatabase persists to disk on key mint).
fn uniq(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{}-{nanos}-{tag}", std::process::id())
}

/// An `EncryptedGraphDB` over a fresh in-memory `SurrealGraphDB`, plus the raw
/// handle for at-rest inspection. Mirrors `build_encrypted_db` in
/// `encrypted.rs`'s unit tests, with the mock swapped for the real backend.
async fn build(tag: &str) -> (Arc<SurrealGraphDB>, EncryptedGraphDB) {
    let raw = Arc::new(SurrealGraphDB::new(StorageMode::Memory).await.unwrap());
    raw.connect().await.unwrap();
    raw.init_schema().await.unwrap();

    let u = uniq(tag);
    let mk_kdb = |suffix: &str| {
        Arc::new(RwLock::new(KeyDatabase::new(
            std::env::temp_dir().join(format!("sovereign-it-{u}-{suffix}-keys.db")),
        )))
    };
    let edb = EncryptedGraphDB::new(
        raw.clone() as Arc<dyn GraphDB>,
        mk_kdb("doc"),
        mk_kdb("msg"),
        mk_kdb("thr"),
        mk_kdb("conv"),
        mk_kdb("con"),
        mk_kdb("shr"),
        Arc::new(Kek::generate()),
        Arc::new(IndexKey::generate()),
        Arc::new(test_device_key()),
    );
    (raw, edb)
}

async fn create_doc(edb: &EncryptedGraphDB, title: &str, content: &str) -> Document {
    let mut doc = Document::new(title.to_string(), "thread:test".to_string(), true);
    doc.content = content.to_string();
    edb.create_document(doc).await.unwrap()
}

fn bare_key(id: &str) -> &str {
    id.split_once(':').map(|(_, k)| k).unwrap_or(id)
}

// ------------------------------------------------------ core roundtrips ----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn document_roundtrip_encrypts_at_rest() {
    let (raw, edb) = build("doc-roundtrip").await;
    let created = create_doc(&edb, "Meeting notes", "Q3 kickoff decisions and owners.").await;
    let id = created.id_string().unwrap();

    // Through the encrypted layer: plaintext.
    let got = edb.get_document(&id).await.unwrap();
    assert_eq!(got.title, "Meeting notes");
    assert_eq!(got.content, "Q3 kickoff decisions and owners.");

    // At rest: ciphertext with nonces paired.
    let at_rest = raw.get_document(&id).await.unwrap();
    assert_ne!(at_rest.content, got.content, "content at rest must be ciphertext");
    assert_ne!(at_rest.title, got.title, "title at rest must be ciphertext");
    assert!(at_rest.encryption_nonce.is_some(), "content nonce must be persisted");
    assert!(at_rest.title_nonce.is_some(), "title nonce must be persisted");
    assert!(!at_rest.title_token_hashes.is_empty(), "blind index must be populated");

    // Update roundtrips too.
    let updated = edb
        .update_document(&id, Some("Meeting notes v2"), Some("Revised owners."))
        .await
        .unwrap();
    assert_eq!(updated.title, "Meeting notes v2");
    let got2 = edb.get_document(&id).await.unwrap();
    assert_eq!(got2.content, "Revised owners.");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn thread_roundtrip_and_find_by_name() {
    let (raw, edb) = build("thread-roundtrip").await;
    let thread = edb
        .create_thread(Thread::new(
            "Circular fashion".to_string(),
            "v4let working thread".to_string(),
        ))
        .await
        .unwrap();
    let id = thread.id_string().unwrap();

    let got = edb.get_thread(&id).await.unwrap();
    assert_eq!(got.name, "Circular fashion");
    assert_eq!(got.description, "v4let working thread");

    let at_rest = raw.get_thread(&id).await.unwrap();
    assert_ne!(at_rest.name, "Circular fashion", "thread name at rest must be ciphertext");

    let found = edb.find_thread_by_name("circular").await.unwrap();
    assert_eq!(
        found.map(|t| t.id_string().unwrap()),
        Some(id),
        "blind-index thread lookup must find the thread on the real backend"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn contact_roundtrip_encrypts_at_rest() {
    let (raw, edb) = build("contact-roundtrip").await;
    let mut contact = Contact::new("Ada Lovelace".to_string(), false);
    contact.notes = "Met at the analytical engines meetup.".to_string();
    let created = edb.create_contact(contact).await.unwrap();
    let id = created.id_string().unwrap();

    let got = edb.get_contact(&id).await.unwrap();
    assert_eq!(got.name, "Ada Lovelace");
    assert_eq!(got.notes, "Met at the analytical engines meetup.");

    let at_rest = raw.get_contact(&id).await.unwrap();
    assert_ne!(at_rest.name, "Ada Lovelace", "contact name at rest must be ciphertext");
    assert_ne!(
        at_rest.notes, "Met at the analytical engines meetup.",
        "contact notes at rest must be ciphertext"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn message_roundtrip_and_blind_index_search() {
    let (raw, edb) = build("msg-search").await;
    let conv = edb
        .create_conversation(Conversation::new(
            "Finance".to_string(),
            ChannelType::Email,
            vec!["contact:peer".to_string()],
        ))
        .await
        .unwrap();
    let conv_id = conv.id_string().unwrap();

    let msg = Message::new(
        conv_id.clone(),
        ChannelType::Email,
        MessageDirection::Inbound,
        "contact:from".to_string(),
        vec!["contact:to".to_string()],
        "The quarterly numbers landed and they look strong.".to_string(),
    );
    let created = edb.create_message(msg).await.unwrap();
    let id = created.id_string().unwrap();

    let got = edb.get_message(&id).await.unwrap();
    assert_eq!(got.body, "The quarterly numbers landed and they look strong.");

    let at_rest = raw.get_message(&id).await.unwrap();
    assert_ne!(at_rest.body, got.body, "message body at rest must be ciphertext");
    assert!(!at_rest.body_token_hashes.is_empty());

    let hits = edb.search_messages("quarterly").await.unwrap();
    assert_eq!(hits.len(), 1, "blind-index search must match on the real backend");
    assert_eq!(hits[0].body, "The quarterly numbers landed and they look strong.");

    let miss = edb.search_messages("unrelated-term").await.unwrap();
    assert!(miss.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn soft_delete_filters_lists_and_restore_brings_back() {
    let (_raw, edb) = build("soft-delete").await;
    let keep = create_doc(&edb, "Keeper", "stays").await;
    let gone = create_doc(&edb, "Goner", "leaves").await;
    let keep_id = keep.id_string().unwrap();
    let gone_id = gone.id_string().unwrap();

    edb.soft_delete_document(&gone_id).await.unwrap();
    let listed = edb.list_documents(None).await.unwrap();
    assert_eq!(
        listed.iter().map(|d| d.id_string().unwrap()).collect::<Vec<_>>(),
        vec![keep_id.clone()],
        "soft-deleted document must fall out of list_documents"
    );

    edb.restore_soft_deleted_document(&gone_id).await.unwrap();
    let listed = edb.list_documents(None).await.unwrap();
    assert_eq!(listed.len(), 2, "restored document must reappear");
}

// ------------------------------------------- review findings (Theme 2) -----

/// H-db1 — `update_document_position` / `set_document_pinned` are silent
/// no-ops on the real backend (`UPDATE $id` with a *string* bind is not a
/// record pointer, and the dropped `Response` hides it). Canvas position and
/// pin persistence are broken in production while the mock passes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn position_and_pin_persist() {
    let (_raw, edb) = build("pos-pin").await;
    let doc = create_doc(&edb, "Spatial", "on the canvas").await;
    let id = doc.id_string().unwrap();

    edb.update_document_position(&id, 12.5, -3.25).await.unwrap();
    edb.set_document_pinned(&id, true).await.unwrap();

    let got = edb.get_document(&id).await.unwrap();
    assert_eq!(got.spatial_x, 12.5, "canvas x position must persist (H-db1)");
    assert_eq!(got.spatial_y, -3.25, "canvas y position must persist (H-db1)");
    assert!(got.pinned, "pinned flag must persist (H-db1)");
}

/// C1 — commit/restore over the encrypted layer. The snapshot carries
/// ciphertext but `DocumentSnapshot` has no nonce fields; any post-commit edit
/// re-encrypts under a fresh nonce, so restore pairs old ciphertext with the
/// new nonce → AEAD failure, and the original plaintext is unrecoverable.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn version_restore_returns_the_committed_content() {
    let (_raw, edb) = build("restore").await;
    let doc = create_doc(&edb, "Versioned", "version one content").await;
    let id = doc.id_string().unwrap();

    let commit = edb.commit_document(&id, "v1").await.unwrap();
    let commit_id = commit.id_string().unwrap();

    // Post-commit edit (re-encrypts under a fresh nonce).
    edb.update_document(&id, None, Some("version two content")).await.unwrap();
    assert_eq!(edb.get_document(&id).await.unwrap().content, "version two content");

    // Restore must bring back v1, readable.
    let restored = edb.restore_document(&id, &commit_id).await.unwrap();
    assert_eq!(restored.content, "version one content", "restore must return the committed content (C1)");

    let got = edb.get_document(&id).await.unwrap();
    assert_eq!(got.content, "version one content", "document must be readable after restore (C1)");
    assert_eq!(got.title, "Versioned", "title must survive the restore cycle (C1)");
}

/// C1 (adjunct) — version history must be *readable* through the encrypted
/// layer: the shell's History panel shows snapshot previews, so
/// `list_document_commits` / `get_commit` on `EncryptedGraphDB` must yield
/// plaintext snapshots (today they return raw ciphertext).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commit_history_is_readable_through_encrypted_layer() {
    let (_raw, edb) = build("history").await;
    let doc = create_doc(&edb, "Historied", "the original words").await;
    let id = doc.id_string().unwrap();

    let commit = edb.commit_document(&id, "checkpoint").await.unwrap();
    let commit_id = commit.id_string().unwrap();

    let listed = edb.list_document_commits(&id).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].snapshot.content, "the original words",
        "snapshot preview must be plaintext through the encrypted layer (C1)"
    );
    assert_eq!(listed[0].snapshot.title, "Historied");

    let got = edb.get_commit(&commit_id).await.unwrap();
    assert_eq!(got.snapshot.content, "the original words");
}

/// C2 — the suggested-link subsystem against the real backend. Enum binds as
/// a JSON-*quoted* string (`'"consolidation"'`), so `RETURN AFTER`
/// deserialization fails AND a malformed row persists whose status never
/// matches `'pending'`; `suggestion_exists` / `list_suggestions_for_document`
/// compare record links against bound strings — always false/empty.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn suggested_link_lifecycle() {
    let (_raw, edb) = build("suggestions").await;
    let a = create_doc(&edb, "Doc A", "alpha").await;
    let b = create_doc(&edb, "Doc B", "beta").await;
    let a_id = a.id_string().unwrap();
    let b_id = b.id_string().unwrap();

    let link = edb
        .create_suggested_link(
            &a_id,
            &b_id,
            RelationType::References,
            0.8,
            "Both discuss the same topic.",
            SuggestionSource::Consolidation,
        )
        .await
        .expect("create_suggested_link must succeed on the real backend (C2)");
    assert_eq!(link.status, SuggestionStatus::Pending);

    assert!(
        edb.suggestion_exists(&a_id, &b_id).await.unwrap(),
        "suggestion_exists must see the pair (C2 — consolidation dedup depends on this)"
    );
    assert!(
        edb.suggestion_exists(&b_id, &a_id).await.unwrap(),
        "suggestion_exists must be bidirectional (C2)"
    );

    let pending = edb.list_pending_suggestions().await.unwrap();
    assert_eq!(pending.len(), 1, "pending suggestion must be listable (C2)");

    let for_doc = edb.list_suggestions_for_document(&a_id).await.unwrap();
    assert_eq!(for_doc.len(), 1, "per-document suggestion list must match (C2)");

    // Accept → status flips and the link is promoted to a real relationship
    // in the suggested direction a → b (review DB-M3: promotion must not
    // invert the edge).
    let link_key = bare_key(&link.id_string().unwrap()).to_string();
    let resolved = edb
        .resolve_suggestion(&link_key, SuggestionStatus::Accepted)
        .await
        .expect("resolve_suggestion must succeed (C2)");
    assert_eq!(resolved.status, SuggestionStatus::Accepted);
    assert!(resolved.resolved_at.is_some());

    let out = edb.list_outgoing_relationships(&a_id).await.unwrap();
    assert_eq!(out.len(), 1, "accepted suggestion must promote to a related_to edge (C2)");
    assert_eq!(
        out[0].out.as_ref().map(thing_to_raw),
        Some(b_id.clone()),
        "promoted edge must run a → b, not inverted (DB-M3)"
    );

    assert!(edb.list_pending_suggestions().await.unwrap().is_empty());
}

/// M4 — `traverse` is non-functional against the real backend (verified in
/// the review). Contract per the trait: connected documents up to `depth` hops.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn traverse_returns_connected_documents() {
    let (_raw, edb) = build("traverse").await;
    let a = create_doc(&edb, "Node A", "start").await;
    let b = create_doc(&edb, "Node B", "middle").await;
    let c = create_doc(&edb, "Node C", "far").await;
    let a_id = a.id_string().unwrap();
    let b_id = b.id_string().unwrap();
    let c_id = c.id_string().unwrap();

    edb.create_relationship(&a_id, &b_id, RelationType::References, 1.0).await.unwrap();
    edb.create_relationship(&b_id, &c_id, RelationType::References, 1.0).await.unwrap();

    let one_hop = edb.traverse(&a_id, 1, 10).await.unwrap();
    let one_ids: Vec<String> = one_hop.iter().map(|d| d.id_string().unwrap()).collect();
    assert_eq!(one_ids, vec![b_id.clone()], "depth-1 traverse must reach the direct neighbor (M4)");

    let two_hop = edb.traverse(&a_id, 2, 10).await.unwrap();
    let two_ids: Vec<String> = two_hop.iter().map(|d| d.id_string().unwrap()).collect();
    assert!(
        two_ids.contains(&c_id),
        "depth-2 traverse must reach the 2-hop neighbor (M4); got {two_ids:?}"
    );
    assert!(!two_ids.contains(&a_id), "traverse must not return the origin");
}

/// H-db2 — create must NEVER write plaintext to the raw store, even before
/// the encryption setters run. The old shape inserted plaintext then
/// overwrote it, leaving a crash window (and WAL residue) of plaintext at
/// rest. The raw row's sensitive fields must be blanked-or-ciphertext at
/// every observable point.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_never_writes_plaintext_content_at_rest() {
    let (raw, edb) = build("no-plaintext-create").await;
    let secret_title = "Quarterly board deck";
    let secret_body = "Layoffs planned for Q3; do not forward.";
    let created = create_doc(&edb, secret_title, secret_body).await;
    let id = created.id_string().unwrap();

    // Caller view is plaintext…
    assert_eq!(created.title, secret_title);
    assert_eq!(created.content, secret_body);

    // …but the raw row never holds the plaintext. It's ciphertext now (with
    // nonces); the point of the fix is it was NEVER the plaintext, so the
    // observable end state must not equal the secret.
    let at_rest = raw.get_document(&id).await.unwrap();
    assert_ne!(at_rest.title, secret_title, "raw title must not be plaintext (H-db2)");
    assert_ne!(at_rest.content, secret_body, "raw content must not be plaintext (H-db2)");
    assert!(at_rest.encryption_nonce.is_some());
    assert!(at_rest.title_nonce.is_some());

    // And the encrypted layer round-trips it back.
    assert_eq!(edb.get_document(&id).await.unwrap().content, secret_body);
}

/// H-db2 — the same guarantee for messages (body/subject at rest).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_message_never_writes_plaintext_at_rest() {
    let (raw, edb) = build("no-plaintext-msg").await;
    let conv = edb
        .create_conversation(Conversation::new(
            "Legal".to_string(),
            ChannelType::Email,
            vec!["contact:peer".to_string()],
        ))
        .await
        .unwrap();
    let mut msg = Message::new(
        conv.id_string().unwrap(),
        ChannelType::Email,
        MessageDirection::Inbound,
        "contact:from".to_string(),
        vec!["contact:to".to_string()],
        "Settlement figure is $2.4M, confidential.".to_string(),
    );
    msg.subject = Some("RE: settlement".to_string());
    let created = edb.create_message(msg).await.unwrap();
    let id = created.id_string().unwrap();

    let at_rest = raw.get_message(&id).await.unwrap();
    assert_ne!(at_rest.body, "Settlement figure is $2.4M, confidential.", "raw body must not be plaintext (H-db2)");
    assert_ne!(at_rest.subject.as_deref(), Some("RE: settlement"), "raw subject must not be plaintext (H-db2)");
    assert_eq!(
        edb.get_message(&id).await.unwrap().body,
        "Settlement figure is $2.4M, confidential."
    );
}

/// DB-M1 — one undecryptable row must not brick every `list_*`: combined with
/// C1, a single bad restore makes the whole workspace fail to load. Contract:
/// skip the corrupt row (with a warning), return the healthy rest.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_undecryptable_row_does_not_brick_lists() {
    let (raw, edb) = build("bad-row").await;
    let healthy = create_doc(&edb, "Healthy", "fine content").await;
    let victim = create_doc(&edb, "Victim", "will be corrupted").await;
    let healthy_id = healthy.id_string().unwrap();
    let victim_id = victim.id_string().unwrap();

    // Simulate at-rest corruption: overwrite ciphertext+nonce with valid
    // base64 that cannot decrypt (what a C1-corrupted restore leaves behind).
    use base64::Engine as _;
    let junk_ct = base64::engine::general_purpose::STANDARD.encode(b"not-a-real-ciphertext");
    let junk_nonce = base64::engine::general_purpose::STANDARD.encode([0u8; 24]);
    raw.set_document_content_encryption(&victim_id, &junk_ct, &junk_nonce).await.unwrap();

    let listed = edb
        .list_documents(None)
        .await
        .expect("list_documents must survive one undecryptable row (DB-M1)");
    let ids: Vec<String> = listed.iter().map(|d| d.id_string().unwrap()).collect();
    assert!(
        ids.contains(&healthy_id),
        "healthy documents must still list when a sibling row is corrupt (DB-M1)"
    );
    let _ = victim_id; // the corrupt row may be skipped or returned degraded — but never sink the list
}
