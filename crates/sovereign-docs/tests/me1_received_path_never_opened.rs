//! ME1 — a received path is never opened.
//!
//! Spec ch. 03, *Received content is hostile — contract (C-MEDIA)*: "**ME1 — A
//! received path is never opened.** A state that arrives whole from outside the
//! device never leads to a read of a path it names. That covers sync, a
//! restored backup, and a document imported as a Sovereign document. A media
//! field in such a state is data, not a location."
//!
//! How a read is observed: the service reads media only through the
//! `MediaFiles` it is given (C-DOC, *What it is given, and nothing else*). The
//! fake records every path it is handed, by either method, and pretends this
//! device holds the named file, so a read would also leave its hash in the
//! stored block. Both are checked; neither depends on timing. A real-file
//! version of the sync case runs over the shipped `FileMedia`
//! (`crates/sovereign-ai/tests/me1_real_files.rs`).
//!
//! Spec v2.0.80 (*The media store*): "A received state's outside paths are
//! neutralised when it is applied, before anything else can see them. This
//! covers sync, a restored backup and a Sovereign import, owned documents
//! included. The block keeps its medium's name and hash, and is shown as
//! « medium not on this device »: absent, not empty. A later save of that
//! document, even by its owner, therefore has no outside path to copy." That is
//! the last test here.
//!
//! These cases are also ME4's received-state call path (`apply_state`, serving
//! `apply_wire` and `restore_backup`), "the application of a received state
//! first".

mod common;

use common::wire::*;
use common::*;
use serde_json::Value;
use sovereign_docs::{BackupDocument, EditSession, MediaPresence, NewContent, NewDocument, Origin, Provenance, RemoteDocument, Trust};

/// The hash the fake gives the named file: what a read would leave behind.
const H: &str = "5ec0e0a5c4a8a1a4d1d55a6f2d1e3c9a4d8a4c07c0a1e7e0f3b9a2c4d6e8f0a1";
/// A network path (on Windows, opening it connects to that host).
const UNC: &str = r"\\attacker.example\share\x.png";
/// The same in the other spelling.
const NET: &str = "//attacker.example/share/x.png";
/// A device file: read whole, it never ends.
const DEV: &str = "/dev/zero";

fn hostile_doc() -> String {
    v2(vec![text_block(1, "received"), image_block(2, UNC, None), image_block(3, NET, None), image_block(4, DEV, None)])
}

fn hold_all(s: &Stack) {
    for p in [UNC, NET, DEV] {
        s.media.put(p, H);
    }
}

/// No hostile path reached the media files, and no stored block carries the
/// hash a read would have produced.
async fn assert_never_opened(s: &Stack, id: &str, what: &str) {
    let asked = s.media.asked();
    for p in [UNC, NET, DEV] {
        assert!(!asked.iter().any(|a| a == p), "{what}: the received path {p:?} was handed to the media files");
    }
    let blocks = assert_v2(&s.stored(id).await.1);
    for b in blocks.iter().filter(|b| b["type"] == "image") {
        assert_ne!(b["ref"]["sha256"], Value::from(H), "{what}: a stored block carries the hash of a received path");
    }
}

fn remote(id: &str, content: String) -> RemoteDocument {
    let mut r = remote_text(id, "Received", "x", "phone");
    r.content = content;
    r
}

// ME1, sync: a new document arriving from a paired device.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me1_apply_wire_never_opens_received_path() {
    let s = Stack::new("me1-wire-new").await;
    hold_all(&s);
    let id = "document:me1new";
    s.svc.apply_wire(remote(id, hostile_doc()), meta(Some(stamp(&s, 0, 0, "phone")), None)).await.unwrap();
    assert_never_opened(&s, id, "apply_wire, new document").await;
}

// ME1, sync: a received state that replaces a local one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me1_apply_wire_update_never_opens_received_path() {
    let s = Stack::new("me1-wire-upd").await;
    hold_all(&s);
    let id = s.create_text("Local", "here").await;
    let prev = current_stamp(&s, &id).await;
    s.media.forget_asked();
    s.svc.apply_wire(remote(&id, hostile_doc()), meta(Some(stamp(&s, 1, 0, "phone")), Some(prev))).await.unwrap();
    let blocks = assert_v2(&s.stored(&id).await.1);
    assert!(blocks.iter().any(|b| b["type"] == "image"), "the received state was applied");
    assert_never_opened(&s, &id, "apply_wire, update").await;
}

// ME1, a restored backup.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me1_restore_backup_never_opens_received_path() {
    let s = Stack::new("me1-backup").await;
    hold_all(&s);
    let id = "document:me1backup";
    let docs = vec![BackupDocument { document: remote(id, hostile_doc()), meta: meta(Some(stamp(&s, 0, 0, "phone")), None) }];
    s.svc.restore_backup(docs, vec![]).await.unwrap();
    assert_never_opened(&s, id, "restore_backup").await;
}

// ME1, a document imported as a Sovereign document (its stored v2 form).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me1_imported_sovereign_document_never_opens_its_paths() {
    let s = Stack::new("me1-import").await;
    hold_all(&s);
    let id = s
        .svc
        .create(NewDocument {
            title: "Imported".into(),
            thread_id: "thread:t1".into(),
            content: NewContent::V2(hostile_doc()),
            trust: Trust::Undetermined,
            provenance: Provenance::Import { source_path: "D:/in/received.sovereign".into() },
            origin: Origin::Import,
            created_at: None,
            modified_at: None,
        })
        .await
        .unwrap()
        .id;
    assert_never_opened(&s, &id, "import of a Sovereign document").await;
}

/// A file on this device that a peer names: its own hash, distinct from the
/// one the peer sent.
const SENTINEL: &str = "C:/Users/someone/Pictures/sentinel.png";
const SENTINEL_SHA: &str = "5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e5e";
/// The hash the peer sent with its medium.
const PEER_SHA: &str = "beefbeefbeefbeefbeefbeefbeefbeefbeefbeefbeefbeefbeefbeefbeefbeef";

// The media store (v2.0.80) and ME1: a received owned document's outside paths are neutralised on apply; its owner's later save reads nothing, and the media show as not on this device.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me1_received_owned_document_is_neutralised_and_its_owners_save_reads_nothing() {
    let s = Stack::new("me1-neutral").await;
    s.media.put(SENTINEL, SENTINEL_SHA); // this device holds the file the peer names
    let id = "document:me1neutral";
    // Owned, from a paired device: one medium with the peer's hash, one from
    // an older peer that sent no hash.
    let content = v2(vec![
        text_block(1, "a paragraph"),
        image_block(2, SENTINEL, Some(PEER_SHA)),
        image_block(3, SENTINEL, None),
    ]);
    let mut r = remote(id, content);
    r.trust = Some(Trust::Owned);
    s.svc.apply_wire(r, meta(Some(stamp(&s, 0, 0, "phone")), None)).await.unwrap();
    assert_eq!(s.svc.read(id).await.unwrap().trust, Trust::Owned, "the received document is hers");

    // Neutralised when applied: no block names the outside path any more.
    for b in assert_v2(&s.stored(id).await.1).iter().filter(|b| b["type"] == "image") {
        assert!(!b.to_string().contains(SENTINEL), "applied: a block still names the outside path");
    }

    // Its owner edits the paragraph and saves, from her editor.
    let mut blocks = assert_v2(&s.stored(id).await.1);
    blocks[0]["text"] = Value::from("a paragraph, edited by its owner");
    s.editor_save(id, 4, v2(blocks)).await.unwrap();
    s.svc.end_session(id, EditSession(4)).await.unwrap();

    assert!(!s.media.asked().iter().any(|a| a == SENTINEL), "the outside path was handed to the media files");
    let images: Vec<Value> =
        assert_v2(&s.stored(id).await.1).into_iter().filter(|b| b["type"] == "image").collect();
    assert_eq!(images.len(), 2, "both media blocks are kept");
    for b in &images {
        assert!(!b.to_string().contains(SENTINEL), "saved: a block names the outside path");
        assert_ne!(b["ref"]["sha256"], Value::from(SENTINEL_SHA), "a block carries the hash of the local file");
    }
    assert_eq!(images[0]["ref"]["sha256"], Value::from(PEER_SHA), "the block keeps its medium's hash");
    assert_eq!(
        s.svc.media_presence(id).await.unwrap(),
        vec![(bid(2), MediaPresence::NotOnThisDevice), (bid(3), MediaPresence::NotOnThisDevice)],
        "both media show as not on this device: absent, not empty"
    );
}

// v2.0.85: a received state kept as a version without being made current (C-WIRE 7, `apply_wire_version`) is never kept with an outside path: held back, or stored with its outside paths neutralised.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me1_a_received_version_is_never_kept_with_an_outside_path() {
    let s = Stack::new("me1-version-kept").await;
    s.media.put(SENTINEL, SENTINEL_SHA);
    let id = s.create_text("Mine", "here").await;
    // No pass has run on this store. A peer relays a concurrent state it lost.
    let content = v2(vec![text_block(1, "the peer's version"), image_block(2, SENTINEL, None)]);
    let out = s.svc.apply_wire_version(remote(&id, content), meta(Some(stamp(&s, 1, 0, "phone")), None)).await;
    assert!(out.is_ok(), "applying a relayed version does not fail");
    for c in s.commits(&id).await {
        assert!(!c.snapshot.content.contains(SENTINEL), "a version is kept with the outside path a peer named");
    }
    for w in s.svc.kept_versions(&id).await.unwrap() {
        assert!(!w.document.content.contains(SENTINEL), "a kept version carries the outside path a peer named");
    }
    assert_eq!(s.media.reads_of(SENTINEL), 0, "the received path was handed to the media files");
}

// v2.0.85: restoring a received version (`restore_version` of a state kept by `apply_wire_version`) reads none of the paths it named.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me1_restoring_a_received_version_reads_none_of_its_paths() {
    let s = Stack::new("me1-version-restore").await;
    s.media.put(SENTINEL, SENTINEL_SHA);
    // The pass has run on this store, so a relayed version is kept (neutralised).
    s.svc.migrate_media_into_store().await.unwrap();
    let id = s.create_text("Mine", "here").await;
    let content = v2(vec![text_block(1, "the peer's version"), image_block(2, SENTINEL, None)]);
    s.svc.apply_wire_version(remote(&id, content), meta(Some(stamp(&s, 1, 0, "phone")), None)).await.unwrap();
    let commit = s
        .commits(&id)
        .await
        .into_iter()
        .find(|c| c.snapshot.content.contains("the peer's version"))
        .expect("the relayed version is kept")
        .id_string()
        .unwrap();
    s.media.forget_asked();
    s.svc.restore_version(&id, &commit).await.unwrap();
    assert_eq!(s.media.reads_of(SENTINEL), 0, "restoring a received version read a path it named");
    let blocks = assert_v2(&s.stored(&id).await.1);
    for b in blocks.iter().filter(|b| b["type"] == "image") {
        assert!(!b.to_string().contains(SENTINEL), "the restored block names the outside path");
        assert_ne!(b["ref"]["sha256"], Value::from(SENTINEL_SHA), "the restored block carries the local file's hash");
    }
}
