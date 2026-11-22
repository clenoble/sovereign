//! ME4 — every caller.
//!
//! Spec ch. 03, *Received content is hostile — contract (C-MEDIA)*: "**ME4 —
//! Every caller.** ME1 to ME3 hold on the seven call paths that hash media
//! today, through `fill_media_hashes`: `create`, `save_locked`,
//! `restore_version`, `apply_state` (both `apply_wire` for sync and
//! `restore_backup`), `convert_one` and `duplicate`. The test covers each one,
//! the application of a received state first. It also covers two sources of
//! hostile paths: a document whose imported content names local files, and a
//! document the assistant writes after reading hostile content."
//!
//! The received state (`apply_state`) is covered first, in
//! `me1_received_path_never_opened.rs`. This file covers the other five, with
//! the two hostile sources. ME2: "A hash is computed only over what the media
//! store holds, named by the store. It is never computed from a path that
//! content supplies." So on none of these paths does a path that hostile
//! content names reach the media files, and no stored block carries the hash a
//! read of it would produce.
//!
//! Not covered here, because the contract does not say it (reported, not
//! settled): whether an editor save of an imported or assistant-written
//! document may copy the paths it names into the store (*The media store*:
//! "copied at its next save on the device that holds them").

mod common;

use common::*;
use serde_json::Value;
use sovereign_docs::{Duplicate, EditContent, EditSession, MediaMark, NewContent, NewDocument, Origin, Provenance, Trust};

const H: &str = "c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00c0ffee00";
/// A local file hostile content names (a key, say).
const LOCAL: &str = "C:/Users/someone/.ssh/id_ed25519";
/// A network path hostile content names.
const NET: &str = "//attacker.example/share/x.png";
/// A picture this device holds, named by an old version with its recorded hash.
const PICTURE: &str = "C:/Users/someone/Pictures/kept.png";
const H_PIC: &str = "d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1d1";

fn hostile_doc(text: &str) -> String {
    v2(vec![text_block(1, text), image_block(2, LOCAL, None), image_block(3, NET, None)])
}

fn hold_all(s: &Stack) {
    for p in [LOCAL, NET] {
        s.media.put(p, H);
    }
}

async fn assert_never_opened(s: &Stack, id: &str, what: &str) {
    let asked = s.media.asked();
    for p in [LOCAL, NET] {
        assert!(!asked.iter().any(|a| a == p), "{what}: the path {p:?} that content names was handed to the media files");
    }
    let blocks = assert_v2(&s.stored(id).await.1);
    for b in blocks.iter().filter(|b| b["type"] == "image") {
        assert_ne!(b["ref"]["sha256"], Value::from(H), "{what}: a stored block carries the hash of a path content named");
    }
}

fn imported(content: String) -> NewDocument {
    NewDocument {
        title: "Imported".into(),
        thread_id: "thread:t1".into(),
        content: NewContent::V2(content),
        trust: Trust::Undetermined,
        provenance: Provenance::Import { source_path: "D:/in/mail.sovereign".into() },
        origin: Origin::Import,
        created_at: None,
        modified_at: None,
    }
}

// ME4, `create`: a document the assistant writes after reading hostile content.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me4_create_by_the_assistant_never_opens_a_path_it_wrote() {
    let s = Stack::new("me4-create").await;
    hold_all(&s);
    let id = s
        .svc
        .create(NewDocument {
            title: "Summary".into(),
            thread_id: "thread:t1".into(),
            content: NewContent::V2(hostile_doc("what the page said")),
            trust: Trust::Owned,
            provenance: Provenance::Assistant,
            origin: Origin::Assistant,
            created_at: None,
            modified_at: None,
        })
        .await
        .unwrap()
        .id;
    assert_never_opened(&s, &id, "create by the assistant").await;
}

// ME4, `save_locked`: the assistant saves a whole document naming local files.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me4_save_by_the_assistant_never_opens_a_path_it_wrote() {
    let s = Stack::new("me4-save").await;
    hold_all(&s);
    let id = s.create_text("Mine", "x").await;
    s.media.forget_asked();
    let base = s.revision(&id).await;
    s.svc
        .save(&id, sovereign_docs::Edit {
            base,
            title: None,
            content: Some(EditContent::V2(hostile_doc("rewritten after reading a page"))),
            origin: Origin::Assistant,
            explicit: false,
            pasted_tokens: vec![],
        })
        .await
        .unwrap();
    assert!(assert_v2(&s.stored(&id).await.1).iter().any(|b| b["type"] == "image"), "the save landed");
    assert_never_opened(&s, &id, "save by the assistant").await;
}

// ME4, `save_locked`: a later write to a document whose imported content names
// local files.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me4_later_writes_to_an_imported_document_never_open_its_paths() {
    let s = Stack::new("me4-import-save").await;
    let id = s.svc.create(imported(hostile_doc("imported"))).await.unwrap().id;
    // The files exist on this device only after the import.
    hold_all(&s);
    s.media.forget_asked();
    s.assistant_save(&id, "the assistant edits the text").await.unwrap();
    let base = s.revision(&id).await;
    s.svc
        .save(&id, sovereign_docs::Edit {
            base,
            title: None,
            content: Some(EditContent::PlainText("a skill edits the text".into())),
            origin: Origin::Skill { name: "tidy".into() },
            explicit: false,
            pasted_tokens: vec![],
        })
        .await
        .unwrap();
    assert_never_opened(&s, &id, "later writes to an imported document").await;
}

// ME4, `restore_version`: a version from before the update, of a document last written on another device, restored after the pass, follows the guarded copy (v2.0.86, Céline: « Plus simple : copier partout »): its network path is refused by name, its block with no recorded hash reads nothing, and a real image with a matching hash is copied and marked.
//
// Rewritten in v6 (the first form's received version can no longer be made),
// then for v2.0.86: an older version restored after the update follows the
// pass's rule, whichever device wrote the document last.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me4_restore_version_of_an_old_version_follows_the_guarded_copy() {
    let s = Stack::new("me4-restore").await;
    hold_all(&s);
    s.media.put(PICTURE, H_PIC);
    let id = s.legacy_authored("Written on the phone before the update", &v2(vec![text_block(1, "now")]), "phone").await;
    // A version from before the update, in the store as legacy data.
    let old = s
        .edb
        .insert_commit(sovereign_db::schema::Commit {
            id: None,
            document_id: id.clone(),
            parent_commit: None,
            message: "Before the update".into(),
            timestamp: chrono::Utc::now(),
            snapshot: sovereign_db::schema::DocumentSnapshot {
                document_id: id.clone(),
                title: "Written on the phone before the update".into(),
                content: v2(vec![
                    text_block(1, "an older version"),
                    image_block(2, LOCAL, None),
                    image_block(3, NET, None),
                    image_block(4, PICTURE, Some(H_PIC)),
                ]),
                thread_id: "thread:t1".into(),
                content_nonce: None,
                title_nonce: None,
                title_token_hashes: vec![],
                deleted_at: None,
            },
            signature: None,
        })
        .await
        .expect("a version from before the update")
        .id_string()
        .unwrap();
    s.svc.migrate_media_into_store().await.unwrap();
    s.media.forget_asked();
    s.svc.restore_version(&id, &old).await.unwrap();
    assert!(assert_v2(&s.stored(&id).await.1).iter().any(|b| b["type"] == "image"), "the version was restored");
    // The network path and the block with no recorded hash: nothing opened.
    assert_never_opened(&s, &id, "restore_version of an old version written elsewhere").await;
    // The real image with its recorded hash: the guarded copy, at most once.
    assert!(s.media.hashed().iter().chain(s.media.copied().iter()).all(|p| p != PICTURE), "only the pass's rule reads it");
    assert!(s.media.verified().iter().filter(|p| *p == PICTURE).count() <= 1, "read in full at most once");
    assert_eq!(s.media.in_store(), vec![(store_name(H_PIC), H_PIC.to_string())], "the image is copied");
    assert_eq!(s.media.mark_of(&store_name(H_PIC)), Some(MediaMark::CopiedByMigration), "and marked « copied by the migration »");
    assert!(!s.stored(&id).await.1.contains(PICTURE), "the restored block names no outside path");
}

// ME4, `convert_one`: the one-time conversion of a legacy document naming files.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me4_conversion_never_opens_a_path_legacy_content_names() {
    let s = Stack::new("me4-convert").await;
    hold_all(&s);
    let id = s
        .legacy(
            "Legacy",
            r#"{"body":"b","images":[{"path":"C:/Users/someone/.ssh/id_ed25519","caption":"c"}],"videos":[{"path":"//attacker.example/share/x.png","caption":"d"}]}"#,
        )
        .await;
    s.svc.migrate_all().await.unwrap();
    assert!(assert_v2(&s.stored(&id).await.1).len() >= 3, "the document was converted");
    let asked = s.media.asked();
    for p in [LOCAL, NET] {
        assert!(!asked.iter().any(|a| a == p), "conversion: the path {p:?} was handed to the media files");
    }
    for b in assert_v2(&s.stored(&id).await.1).iter().filter(|b| b["type"] != "text") {
        assert_ne!(b["ref"]["sha256"], Value::from(H), "conversion: a stored block carries the hash of a path content named");
    }
}

// ME4, `duplicate`: a copy of an imported document naming local files.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn me4_duplicate_never_opens_a_path_the_source_names() {
    let s = Stack::new("me4-duplicate").await;
    let src = s.svc.create(imported(hostile_doc("imported"))).await.unwrap().id;
    hold_all(&s);
    s.media.forget_asked();
    let copy = s
        .svc
        .duplicate(Duplicate { source: src.clone(), title: None, thread_id: None, origin: Origin::Editor { session: EditSession(3) } })
        .await
        .unwrap()
        .id;
    assert_never_opened(&s, &copy, "duplicate").await;
    assert_never_opened(&s, &src, "duplicate (source)").await;
}
