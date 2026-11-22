//! The media store, as tests can see it from the service.
//!
//! Spec ch. 03, *Received content is hostile — contract (C-MEDIA)*, *The media
//! store*, as of v2.0.81 (Céline, 2026-11-13: « oui copie chiffrée des médias
//! dans sovereign »):
//! - "Inserting a medium copies it into the store. After the copy, the block
//!   names the store's medium, its name in the store and its hash, and no
//!   longer the outside path. Insertion is the only gesture that copies an
//!   outside path."
//! - "The documents that exist at the update are migrated once, on each
//!   device, before any sync is applied." For every media block that still
//!   names an outside path: ME2's lexical refusals first; a document whose
//!   current state was written on another device (`author_device`) is
//!   neutralised, whatever the file; otherwise its first bytes are read first,
//!   and a file that is not an image or a video is neutralised there, counted
//!   and said, and never read further; then the file is read in a stream under
//!   ME3's cap and its hash compared with the recorded one: copied on a match,
//!   neutralised otherwise. "Every medium the pass copies is marked « copied by
//!   the migration » in the store. A medium inserted after the update has no
//!   mark." "After the pass, no outside path is left."
//! - "A medium must be a medium: the pass, and insertion too, copy a file only
//!   if its first bytes say image or video, never by its name."
//! - "A neutralised block resolves by its hash in the local store."
//! - The spec's three tests: the round trip; the legacy path, last written
//!   elsewhere; the residual, last written here (Céline, 2026-11-13: « Copier,
//!   avec les garde-fous »). Spec v2.0.81 final, `v0013/spec-2081` 0662ff3.
//!
//! And ME2: "A hash is computed only over what the media store holds [...]
//! never computed from a path that content supplies."
//!
//! Which field of the block holds the store's name is the implementer's; these
//! tests require that the name and the hash are in the block and the outside
//! path is not. The copy and the store go through `MediaFiles` (the fake
//! pretends the device holds the files and records every path it is handed).
//! Encryption and the persona's area are behind that seam: C-AREA's walk covers
//! them.
//!
//! v2.0.84 adds the record of what the pass neutralises (document, block, former
//! path as text, reason), read with `DocumentService::neutralised_media`. "Before
//! any sync is applied" is tested at the service, where every received state
//! enters (`apply_wire`), whatever order the hosts start things in: the hosts
//! spawn the unlock migration in the background and start sync right after, so
//! an order kept only by the hosts would race.

mod common;

use common::wire::*;
use common::*;
use serde_json::Value;
use sovereign_docs::{
    Duplicate, EditSession, MediaMark, MediaMigrationReport, MediaPresence, NeutralisedReason, Origin, SVG_SNIFF_BYTES,
};

const H: &str = "0a0b0c0d0a0b0c0d0a0b0c0d0a0b0c0d0a0b0c0d0a0b0c0d0a0b0c0d0a0b0c0d";
/// A picture the user inserts from her own disk.
const OUTSIDE: &str = "C:/Users/someone/Pictures/cat.png";

fn image_of(stored: &str) -> Value {
    assert_v2(stored).into_iter().find(|b| b["type"] == "image").expect("an image block")
}

// The media store: inserting a medium copies it into the store; its hash is the copy's (ME2).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_inserting_a_medium_copies_it_into_the_store() {
    let s = Stack::new("store-insert").await;
    s.media.put(OUTSIDE, H);
    let id = s.create_text("Mine", "x").await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "x"), image_block(2, OUTSIDE, None)])).await.unwrap();
    assert_eq!(s.media.copied(), vec![OUTSIDE.to_string()], "the inserted medium is copied into the store, once");
    assert!(
        !s.media.hashed().iter().any(|p| p == OUTSIDE),
        "ME2: no hash is computed from the path the content supplies"
    );
    let img = image_of(&s.stored(&id).await.1);
    assert!(!img.to_string().contains(OUTSIDE), "after the copy the block no longer names the outside path");
    assert!(img.to_string().contains(&store_name(H)), "after the copy the block names the store's medium");
    assert_eq!(img["ref"]["sha256"], Value::from(H), "the block carries the hash of the store's copy");
    assert_eq!(img["alt"], "a cat", "nothing else of the block changes");
    assert_eq!(
        s.svc.media_presence(&id).await.unwrap(),
        vec![(bid(2), MediaPresence::InStore)],
        "the copied medium is in this device's store"
    );
    assert_eq!(s.media.mark_of(&store_name(H)), None, "a medium inserted after the update has no mark");
}

// The media store: the original is not read again after insertion, whatever writes follow.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_original_is_not_read_again_after_insertion() {
    let s = Stack::new("store-not-again").await;
    s.media.put(OUTSIDE, H);
    let id = s.create_text("Mine", "x").await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "x"), image_block(2, OUTSIDE, None)])).await.unwrap();
    s.media.forget_asked();

    // An editor save of what the editor holds now, with a text change.
    let mut blocks = assert_v2(&s.stored(&id).await.1);
    blocks[0]["text"] = Value::from("x and more");
    s.editor_save(&id, 1, v2(blocks)).await.unwrap();
    s.svc.end_session(&id, EditSession(1)).await.unwrap();
    s.assistant_save(&id, "the assistant rewrites the text").await.unwrap();
    let with_image = s
        .commits(&id)
        .await
        .into_iter()
        .find(|c| c.snapshot.content.contains("\"image\""))
        .expect("a version holding the image")
        .id_string()
        .unwrap();
    s.svc.restore_version(&id, &with_image).await.unwrap();
    s.svc
        .duplicate(Duplicate { source: id.clone(), title: None, thread_id: None, origin: Origin::Editor { session: EditSession(2) } })
        .await
        .unwrap();
    s.svc.migrate_all().await.unwrap();

    assert!(!s.media.asked().iter().any(|p| p == OUTSIDE), "the original was read again after insertion");
    assert!(s.media.copied().is_empty(), "a medium already in the store is not copied again");
}

/// Saves the document as its owner, from her editor, with its first text
/// block edited.
async fn owner_edits(s: &Stack, id: &str, session: u64, text: &str) {
    let mut blocks = assert_v2(&s.stored(id).await.1);
    let i = blocks.iter().position(|b| b["type"] == "text").expect("a text block");
    blocks[i]["text"] = Value::from(text);
    s.editor_save(id, session, v2(blocks)).await.unwrap();
    s.svc.end_session(id, EditSession(session)).await.unwrap();
}

const H1: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const H2: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const H3: &str = "3333333333333333333333333333333333333333333333333333333333333333";
const H4: &str = "4444444444444444444444444444444444444444444444444444444444444444";
const H5: &str = "5555555555555555555555555555555555555555555555555555555555555555";
const H6: &str = "6666666666666666666666666666666666666666666666666666666666666666";
const H7: &str = "9999999999999999999999999999999999999999999999999999999999999999";

// The media store, the migration pass: first bytes first, then a matching file is copied and marked; every other outside path is neutralised and counted; no outside path is left.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_migration_pass_leaves_no_outside_path() {
    let s = Stack::new("store-pass").await;
    let matching = "C:/Users/someone/Pictures/a.png";
    let other = "C:/Users/someone/Pictures/b.png";
    let gone = "C:/Users/someone/Pictures/gone.png";
    let network = "//server/share/n.png";
    s.media.put(matching, H1); // the hash the block recorded
    s.media.put(other, H3); // the file changed since: the block recorded H2
    s.media.put(network, H5); // a network path the pass must not open
    // A row from before the update with no recorded author: written here.
    let id = s
        .legacy(
            "From before the update",
            &v2(vec![
                text_block(1, "x"),
                image_block(2, matching, Some(H1)),
                image_block(3, other, Some(H2)),
                image_block(4, gone, Some(H4)),
                image_block(5, network, Some(H5)),
            ]),
        )
        .await;
    let report = s.svc.migrate_media_into_store().await.unwrap();
    assert_eq!(report, MediaMigrationReport { copied: 1, neutralised: 3 }, "one copied; three neutralised, counted");
    let stored = s.stored(&id).await.1;
    for p in [matching, other, gone, network] {
        assert!(!stored.contains(p), "after the pass no outside path is left: {p:?}");
    }
    assert_eq!(s.media.reads_of(network), 0, "ME2: a network path is refused by its name, never opened");
    assert!(
        s.media.hashed().is_empty() && s.media.copied().is_empty(),
        "the pass reads only its first bytes and its checked copy"
    );
    assert!(s.media.sniffed().iter().any(|(p, _)| p == matching), "the first bytes are read first");
    let images: Vec<Value> = assert_v2(&stored).into_iter().filter(|b| b["type"] == "image").collect();
    assert!(images[0].to_string().contains(&store_name(H1)), "the copied block names the store's medium");
    for (img, h) in images.iter().zip([H1, H2, H4, H5]) {
        assert_eq!(img["ref"]["sha256"], Value::from(h), "every block keeps its recorded hash");
    }
    assert_eq!(s.media.in_store(), vec![(store_name(H1), H1.to_string())], "only the matching file is copied");
    assert_eq!(s.media.mark_of(&store_name(H1)), Some(MediaMark::CopiedByMigration), "and marked « copied by the migration »");
    assert_eq!(
        s.svc.media_presence(&id).await.unwrap(),
        vec![
            (bid(2), MediaPresence::InStore),
            (bid(3), MediaPresence::NotOnThisDevice),
            (bid(4), MediaPresence::NotOnThisDevice),
            (bid(5), MediaPresence::NotOnThisDevice),
        ]
    );
}

/// A file on this device that a document from before the update names.
const SENTINEL: &str = "C:/Users/someone/Pictures/sentinel.png";

// The media store, the legacy path, last written elsewhere (v2.0.86, Céline: « Plus simple : copier partout »): a document applied before the update whose current state was written on another device names a sentinel; the guarded copy holds for it as for any document: the sentinel is read at most once, by the pass, copied and marked, and never read again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_legacy_path_last_written_elsewhere_gets_the_guarded_copy() {
    let s = Stack::new("store-legacy-elsewhere").await;
    s.media.put(SENTINEL, H1); // the file is here, a medium, with the very hash the block recorded
    let id = s
        .legacy_authored("Applied before the update", &v2(vec![text_block(1, "x"), image_block(2, SENTINEL, Some(H1))]), "phone")
        .await;
    assert_eq!(
        s.svc.stored_state(&id).await.unwrap().meta.author_device.as_deref(),
        Some("phone"),
        "precondition: its current state was written on another device"
    );
    let report = s.svc.migrate_media_into_store().await.unwrap();
    assert_eq!(report, MediaMigrationReport { copied: 1, neutralised: 0 }, "copied, whichever device wrote it last");
    read_at_most_once_by_the_pass(&s, SENTINEL);
    assert_eq!(s.media.in_store(), vec![(store_name(H1), H1.to_string())], "copied into the store");
    assert_eq!(s.media.mark_of(&store_name(H1)), Some(MediaMark::CopiedByMigration), "marked « copied by the migration »");
    let img = image_of(&s.stored(&id).await.1);
    assert!(!img.to_string().contains(SENTINEL), "no outside path is left");
    assert!(img.to_string().contains(&store_name(H1)), "the block names the store's medium");
    assert_eq!(s.svc.media_presence(&id).await.unwrap(), vec![(bid(2), MediaPresence::InStore)]);
    s.media.forget_asked();
    owner_edits(&s, &id, 1, "edited by its owner after the update").await;
    assert_eq!(s.media.reads_of(SENTINEL), 0, "never read again");
}

/// Whatever the pass read of `path` was read once at most, and by the pass
/// alone: one first-bytes read, one checked full read, nothing else.
fn read_at_most_once_by_the_pass(s: &Stack, path: &str) {
    let sniffs = s.media.sniffed().iter().filter(|(p, _)| p == path).count();
    let full = s.media.verified().iter().filter(|p| *p == path).count();
    assert!(sniffs <= 1, "its first bytes are read at most once ({sniffs})");
    assert!(full <= 1, "it is read in full at most once ({full})");
    assert!(
        !s.media.hashed().iter().chain(s.media.copied().iter()).any(|p| p == path),
        "only the pass reads it: no hash or insertion copy from the path"
    );
}

// The media store, the residual, last written here (one of the spec's three tests; Céline: « Copier, avec les garde-fous »): the sentinel is read at most once, by the pass; it is a medium and its hash matches, so it is copied and marked « copied by the migration »; it is never read again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_residual_last_written_here_is_read_once_by_the_pass_copied_and_marked() {
    let s = Stack::new("store-residual").await;
    s.media.put(SENTINEL, H1); // a medium; the hash the block holds was filled from it
    let id = s
        .legacy_authored("Received, then saved here", &v2(vec![text_block(1, "x"), image_block(2, SENTINEL, Some(H1))]), DEVICE)
        .await;
    let report = s.svc.migrate_media_into_store().await.unwrap();
    assert_eq!(report, MediaMigrationReport { copied: 1, neutralised: 0 });
    read_at_most_once_by_the_pass(&s, SENTINEL);
    assert_eq!(s.media.verified(), vec![SENTINEL.to_string()], "the pass read it to copy it");
    assert_eq!(s.media.in_store(), vec![(store_name(H1), H1.to_string())], "it is copied into the store");
    assert_eq!(s.media.mark_of(&store_name(H1)), Some(MediaMark::CopiedByMigration), "marked « copied by the migration »");
    let img = image_of(&s.stored(&id).await.1);
    assert!(!img.to_string().contains(SENTINEL), "no outside path is left");
    assert!(img.to_string().contains(&store_name(H1)), "the block names the store's medium");
    assert_eq!(s.svc.media_presence(&id).await.unwrap(), vec![(bid(2), MediaPresence::InStore)]);

    // Never read again: an owner's save, an assistant write, a duplicate.
    s.media.forget_asked();
    owner_edits(&s, &id, 1, "edited by its owner after the update").await;
    s.assistant_save(&id, "the assistant rewrites the text").await.unwrap();
    s.svc
        .duplicate(Duplicate { source: id.clone(), title: None, thread_id: None, origin: Origin::Editor { session: EditSession(2) } })
        .await
        .unwrap();
    assert_eq!(s.media.reads_of(SENTINEL), 0, "the sentinel is never read again");
}

// The media store, the residual of another hash: read at most once, by the pass, never copied; the block is neutralised.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_residual_of_another_hash_is_neutralised_and_never_copied() {
    let s = Stack::new("store-residual-other").await;
    s.media.put(SENTINEL, H3); // a medium, but not the hash the block recorded
    let id = s
        .legacy_authored("Written here before the update", &v2(vec![text_block(1, "x"), image_block(2, SENTINEL, Some(H1))]), DEVICE)
        .await;
    let report = s.svc.migrate_media_into_store().await.unwrap();
    owner_edits(&s, &id, 1, "edited by its owner after the update").await;
    assert_eq!(report, MediaMigrationReport { copied: 0, neutralised: 1 });
    read_at_most_once_by_the_pass(&s, SENTINEL);
    assert!(s.media.in_store().is_empty(), "a file of another hash is never copied");
    let img = image_of(&s.stored(&id).await.1);
    assert!(!img.to_string().contains(SENTINEL), "the block is neutralised");
    assert_eq!(img["ref"]["sha256"], Value::from(H1), "it keeps the hash it recorded");
    assert_eq!(s.svc.media_presence(&id).await.unwrap(), vec![(bid(2), MediaPresence::NotOnThisDevice)]);
}

// The media store, "a medium must be a medium", in the pass: a non-medium at a path a peer may have named is read for its first bytes only, never further, never hashed, never copied, even when the recorded hash would match; it is neutralised and counted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_residual_not_a_medium_is_read_for_its_first_bytes_only() {
    let s = Stack::new("store-residual-not-medium").await;
    s.media.put_not_medium(SENTINEL, H1); // a text file renamed .png, of the very hash the block recorded
    let id = s
        .legacy_authored("Received, then saved here", &v2(vec![text_block(1, "x"), image_block(2, SENTINEL, Some(H1))]), DEVICE)
        .await;
    let report = s.svc.migrate_media_into_store().await.unwrap();
    owner_edits(&s, &id, 1, "edited by its owner after the update").await;
    assert_eq!(report, MediaMigrationReport { copied: 0, neutralised: 1 }, "neutralised and counted");
    let sniffs: Vec<(String, usize)> = s.media.sniffed().into_iter().filter(|(p, _)| p == SENTINEL).collect();
    assert_eq!(sniffs.len(), 1, "its first bytes are read, once");
    assert!(sniffs[0].1 <= SVG_SNIFF_BYTES, "no more than the first bytes, within the stated bound ({} asked)", sniffs[0].1);
    assert!(s.media.verified().is_empty(), "never read further: no full read, no hash");
    assert!(s.media.hashed().is_empty() && s.media.copied().is_empty());
    assert!(s.media.in_store().is_empty(), "never copied");
    let img = image_of(&s.stored(&id).await.1);
    assert!(!img.to_string().contains(SENTINEL), "the block is neutralised");
    assert_eq!(s.svc.media_presence(&id).await.unwrap(), vec![(bid(2), MediaPresence::NotOnThisDevice)]);
}

// The media store, the round trip (the spec's named test): A's medium goes to B and back; on A it is shown, and no path named by a received state was read.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_round_trip_shows_the_medium_on_a_and_reads_no_received_path() {
    let a = Stack::with_device("store-trip-a", "desktop").await;
    let b = Stack::with_device("store-trip-b", "phone").await;
    a.media.put(OUTSIDE, H);
    b.media.put(OUTSIDE, H); // B has a file at the same path: a read there would show
    let id = a.create_text("Mine", "x").await;
    a.editor_save(&id, 1, v2(vec![text_block(1, "x"), image_block(2, OUTSIDE, None)])).await.unwrap();
    a.svc.end_session(&id, EditSession(1)).await.unwrap();

    send(&a, &b, &id).await;
    owner_edits(&b, &id, 2, "edited on the phone").await;
    a.media.forget_asked();
    send(&b, &a, &id).await;

    assert_eq!(texts(&a.stored(&id).await.1)[0], "edited on the phone", "B's edit came back");
    assert_eq!(b.media.reads_of(OUTSIDE), 0, "B never read a path A's state named");
    assert!(a.media.asked().is_empty(), "A read no path named by the state it received back");
    for s in [&a, &b] {
        let img = image_of(&s.stored(&id).await.1);
        assert!(!img.to_string().contains(OUTSIDE), "{}: no block names the outside path", s.device);
        assert_eq!(img["ref"]["sha256"], Value::from(H), "{}: the block keeps its medium's hash", s.device);
    }
    assert_eq!(a.svc.media_presence(&id).await.unwrap(), vec![(bid(2), MediaPresence::InStore)], "on A the medium is shown");
    assert_eq!(
        b.svc.media_presence(&id).await.unwrap(),
        vec![(bid(2), MediaPresence::NotOnThisDevice)],
        "on B it is not on this device: absent, not empty"
    );
}

/// A text file renamed `.png`: its first bytes say neither image nor video.
const RENAMED: &str = "C:/Users/someone/Documents/notes.png";
const H_TEXT: &str = "7777777777777777777777777777777777777777777777777777777777777777";

// The media store, "a medium must be a medium" (v2.0.81): insertion copies a file only if its first bytes say image or video; a text file renamed .png is never copied, and the block is neutralised.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_insertion_never_copies_a_file_that_is_not_a_medium() {
    let s = Stack::new("store-not-medium-insert").await;
    s.media.put_not_medium(RENAMED, H_TEXT);
    let id = s.create_text("Mine", "x").await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "x"), image_block(2, RENAMED, None)])).await.unwrap();
    assert!(s.media.in_store().is_empty(), "a file that is not an image or a video is never copied");
    assert!(!s.media.hashed().iter().any(|p| p == RENAMED), "ME2: no hash computed from the path");
    let img = image_of(&s.stored(&id).await.1);
    assert!(!img.to_string().contains(RENAMED), "the block is neutralised");
    assert_ne!(img["ref"]["sha256"], Value::from(H_TEXT), "the block carries no hash of that file");
    assert_eq!(s.svc.media_presence(&id).await.unwrap(), vec![(bid(2), MediaPresence::NotOnThisDevice)]);
}

// The media store, the record (v2.0.84, Céline: « Liste large + registre »; v2.0.85): what the pass neutralises it records — document, block, former path as text, reason — one entry per neutralised block, all seven reasons here once each (a file that is there is never « absent »; v2.0.86: « no recorded hash » in place of « written elsewhere »), and nothing ever opens a path from it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_migration_pass_records_what_it_neutralises() {
    let s = Stack::new("store-record").await;
    let matching = "C:/Users/someone/Pictures/a.png";
    let other = "C:/Users/someone/Pictures/b.png";
    let gone = "C:/Users/someone/Pictures/gone.png";
    let network = "//server/share/n.png";
    s.media.put(matching, H1);
    s.media.put(other, H3); // the block recorded H2
    s.media.put(network, H5);
    s.media.put_not_medium(RENAMED, H_TEXT);
    s.media.put(SENTINEL, H1);
    let big = "C:/Users/someone/Videos/holiday.png";
    let locked = "C:/Users/someone/Pictures/locked.png";
    s.media.put_too_large(big, H6); // there, an image, the recorded hash, above the cap
    s.media.put_unreadable(locked); // there, but a read fails (not « not found »)
    let here = s
        .legacy(
            "Written here before the update",
            &v2(vec![
                text_block(1, "x"),
                image_block(2, matching, Some(H1)),
                image_block(3, other, Some(H2)),
                image_block(4, gone, Some(H4)),
                image_block(5, network, Some(H5)),
                image_block(6, RENAMED, Some(H_TEXT)),
                image_block(8, big, Some(H6)),
                image_block(9, locked, Some(H7)),
            ]),
        )
        .await;
    // Written on another device, with a block that recorded no hash: neutralised
    // for the missing hash, not for the device; nothing is read for it.
    let elsewhere = s
        .legacy_authored("Written on the phone before the update", &v2(vec![text_block(1, "y"), image_block(7, SENTINEL, None)]), "phone")
        .await;

    let report = s.svc.migrate_media_into_store().await.unwrap();
    assert_eq!(report, MediaMigrationReport { copied: 1, neutralised: 7 });
    assert_eq!(s.media.reads_of(SENTINEL), 0, "nothing is read for a block with no recorded hash");
    let record = s.svc.neutralised_media().await.unwrap();
    assert_eq!(record.len(), report.neutralised, "the record stands behind the « neutralised » count");
    let mut got: Vec<(String, sovereign_docs::BlockId, String, NeutralisedReason)> =
        record.into_iter().map(|e| (e.document, e.block, e.former_path, e.reason)).collect();
    got.sort_by(|a, b| (&a.0, a.3).cmp(&(&b.0, b.3)));
    let mut want = vec![
        (here.clone(), bid(3), other.to_string(), NeutralisedReason::OtherHash),
        (here.clone(), bid(4), gone.to_string(), NeutralisedReason::Absent),
        (here.clone(), bid(5), network.to_string(), NeutralisedReason::LexicalRefusal),
        (here.clone(), bid(6), RENAMED.to_string(), NeutralisedReason::NotAMedium),
        (here.clone(), bid(8), big.to_string(), NeutralisedReason::TooLarge),
        (here.clone(), bid(9), locked.to_string(), NeutralisedReason::Unreadable),
        (elsewhere.clone(), bid(7), SENTINEL.to_string(), NeutralisedReason::NoRecordedHash),
    ];
    want.sort_by(|a, b| (&a.0, a.3).cmp(&(&b.0, b.3)));
    assert_eq!(got, want, "each neutralised block, with its former path as text and its reason");

    // Nothing ever opens a path from the record: reading it, and the owner's
    // later saves of both documents, read none of the former paths.
    s.media.forget_asked();
    s.svc.neutralised_media().await.unwrap();
    owner_edits(&s, &here, 1, "edited after the update").await;
    owner_edits(&s, &elsewhere, 2, "edited after the update").await;
    s.svc.neutralised_media().await.unwrap();
    for p in [other, gone, network, RENAMED, SENTINEL, big, locked] {
        assert_eq!(s.media.reads_of(p), 0, "a path in the record was opened: {p:?}");
    }
}

// The media store: "The documents that exist at the update are migrated once, on each device, before any sync is applied." No received state is applied on a store the pass has not run on: either the pass has run by then, or the state is not applied.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_no_received_state_is_applied_before_the_pass() {
    let s = Stack::new("store-before-sync").await;
    s.media.put(SENTINEL, H1);
    let legacy = s
        .legacy_authored("Written here before the update", &v2(vec![text_block(1, "x"), image_block(2, SENTINEL, Some(H1))]), DEVICE)
        .await;
    // No pass has run on this store. A paired device's state arrives.
    let id = "document:arrives-first";
    let out = s.svc.apply_wire(remote_text(id, "From the phone", "x", "phone"), meta(Some(stamp(&s, 0, 0, "phone")), None)).await;
    let applied = matches!(&out, Ok(o) if matches!(o.applied, sovereign_docs::WireApplied::Applied(_)));
    if applied {
        assert!(
            !s.stored(&legacy).await.1.contains(SENTINEL),
            "a received state was applied while the migration pass had not run on this store"
        );
    } else {
        assert!(s.edb.get_document(id).await.is_err(), "a received state held back before the pass stores nothing");
    }
}

// The media store, the record (v2.0.84 final): "The record lives in the persona's area, encrypted, outside every synced store: it never leaves the device." What sync and backup carry holds no record entry, and a device that receives them has no record.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_the_record_never_leaves_the_device() {
    let s = Stack::with_device("store-record-stays", "desktop").await;
    let other = "C:/Users/someone/Pictures/b.png";
    let gone = "C:/Users/someone/Pictures/gone.png";
    let network = "//server/share/n.png";
    s.media.put(other, H3); // the block recorded H2
    s.media.put_not_medium(RENAMED, H_TEXT);
    s.media.put(SENTINEL, H1);
    let here = s
        .legacy(
            "Written here before the update",
            &v2(vec![
                text_block(1, "x"),
                image_block(3, other, Some(H2)),
                image_block(4, gone, Some(H4)),
                image_block(5, network, Some(H5)),
                image_block(6, RENAMED, Some(H_TEXT)),
            ]),
        )
        .await;
    let elsewhere = s
        .legacy_authored("Written on the phone before the update", &v2(vec![text_block(1, "y"), image_block(7, SENTINEL, None)]), "phone")
        .await;
    s.svc.migrate_media_into_store().await.unwrap();
    assert_eq!(s.svc.neutralised_media().await.unwrap().len(), 5, "precondition: the record holds five entries");
    let former = [other, gone, network, RENAMED, SENTINEL];

    // What sync sends and a backup holds: the documents' stored states, and the purge markers.
    let states = s.svc.stored_states().await.unwrap();
    let mut ids: Vec<String> = states.iter().map(|w| w.document.id.clone()).collect();
    ids.sort();
    let mut want = vec![here.clone(), elsewhere.clone()];
    want.sort();
    assert_eq!(ids, want, "sync and backup carry the two documents and nothing else");
    for w in &states {
        let d = &w.document;
        let provenance = d.provenance.as_ref().map(sovereign_docs::encode_provenance).unwrap_or_default();
        for p in former {
            for (field, text) in [("content", &d.content), ("title", &d.title), ("thread", &d.thread_id), ("provenance", &provenance)] {
                assert!(!text.contains(p), "a synced state's {field} carries a recorded path: {p:?}");
            }
        }
    }
    let markers = s.svc.purge_markers().await.unwrap();
    assert!(markers.is_empty(), "no purge marker carries anything here");

    // A device that receives them, by sync or by a restored backup, has no record.
    let phone = Stack::with_device("store-record-phone", "phone").await;
    send(&s, &phone, &here).await;
    send(&s, &phone, &elsewhere).await;
    assert!(phone.svc.neutralised_media().await.unwrap().is_empty(), "the record does not travel by sync");
    let restored = Stack::with_device("store-record-restored", "laptop2").await;
    let backup = states.into_iter().map(|w| sovereign_docs::BackupDocument { document: w.document, meta: w.meta }).collect();
    restored.svc.restore_backup(backup, markers).await.unwrap();
    assert!(restored.svc.neutralised_media().await.unwrap().is_empty(), "the record does not travel by backup");
}

/// An XML password list saved under an `.svg` name.
const PASSWORDS_SVG: &str = "C:/Users/someone/Documents/passwords.svg";
const PASSWORDS_HEAD: &[u8] = b"<?xml version=\"1.0\"?>\n<passwords><entry site=\"bank\">hunter2</entry></passwords>\n";
const H_PASSWORDS: &str = "8888888888888888888888888888888888888888888888888888888888888888";

// v2.0.84: "An XML file with another root is not a medium, whatever its name." An XML password list named .svg is never copied, at insertion or by the pass (the fake decides with the contract's own `is_medium`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_an_xml_file_named_svg_is_never_copied_at_insertion_or_by_the_pass() {
    // At insertion.
    let s = Stack::new("store-xml-svg-insert").await;
    s.media.put_with_head(PASSWORDS_SVG, H_PASSWORDS, PASSWORDS_HEAD);
    let id = s.create_text("Mine", "x").await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "x"), image_block(2, PASSWORDS_SVG, None)])).await.unwrap();
    assert!(s.media.in_store().is_empty(), "insertion: an XML file of another root is never copied");
    assert!(!s.stored(&id).await.1.contains(PASSWORDS_SVG), "insertion: the block is neutralised");

    // By the pass.
    let p = Stack::new("store-xml-svg-pass").await;
    p.media.put_with_head(PASSWORDS_SVG, H_PASSWORDS, PASSWORDS_HEAD);
    let doc = p
        .legacy_authored("Written here before the update", &v2(vec![text_block(1, "x"), image_block(2, PASSWORDS_SVG, Some(H_PASSWORDS))]), DEVICE)
        .await;
    let report = p.svc.migrate_media_into_store().await.unwrap();
    assert_eq!(report, MediaMigrationReport { copied: 0, neutralised: 1 });
    assert!(p.media.in_store().is_empty(), "the pass: an XML file of another root is never copied");
    assert!(p.media.verified().is_empty(), "the pass: read for its first bytes only");
    let record = p.svc.neutralised_media().await.unwrap();
    assert_eq!(record.len(), 1);
    assert_eq!((record[0].document.as_str(), record[0].reason), (doc.as_str(), NeutralisedReason::NotAMedium));
}

/// Seeds a version from before the update in the store, as legacy data
/// (sealed and MAC'd by the encrypted layer), holding `content`.
async fn seed_old_version(s: &Stack, id: &str, content: String) -> String {
    s.edb
        .insert_commit(sovereign_db::schema::Commit {
            id: None,
            document_id: id.to_string(),
            parent_commit: None,
            message: "Before the update".into(),
            timestamp: chrono::Utc::now(),
            snapshot: sovereign_db::schema::DocumentSnapshot {
                document_id: id.to_string(),
                title: "Written here before the update".into(),
                content,
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
        .unwrap()
}

// v2.0.86: an older version, restored after the pass, follows the pass's guarded copy whichever device wrote the document last (here: another device) — a real image with a matching hash is read at most once, copied, marked « copied by the migration », and the restored block names the store's medium.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_restoring_an_old_version_copies_a_matching_medium_and_marks_it_whoever_wrote_it() {
    let s = Stack::new("store-old-version-copy").await;
    s.media.put(SENTINEL, H1);
    let id = s.legacy_authored("Written on the phone before the update", &v2(vec![text_block(1, "now")]), "phone").await;
    let old = seed_old_version(&s, &id, v2(vec![text_block(1, "then"), image_block(2, SENTINEL, Some(H1))])).await;
    s.svc.migrate_media_into_store().await.unwrap();
    s.media.forget_asked();
    s.svc.restore_version(&id, &old).await.unwrap();
    read_at_most_once_by_the_pass(&s, SENTINEL);
    assert_eq!(s.media.in_store(), vec![(store_name(H1), H1.to_string())], "copied into the store");
    assert_eq!(s.media.mark_of(&store_name(H1)), Some(MediaMark::CopiedByMigration), "marked « copied by the migration »");
    let img = image_of(&s.stored(&id).await.1);
    assert!(!img.to_string().contains(SENTINEL), "the restored block names no outside path");
    assert!(img.to_string().contains(&store_name(H1)), "the restored block names the store's medium");
    assert_eq!(s.svc.media_presence(&id).await.unwrap(), vec![(bid(2), MediaPresence::InStore)]);
}

// v2.0.86: an older version, restored after the pass, whichever device wrote the document last (here: another device) — a non-medium (read for its first bytes only), a file of another hash and a block with no recorded hash (nothing read) are neutralised and recorded with their reasons, never copied.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_restoring_an_old_version_neutralises_and_records_the_rest_whoever_wrote_it() {
    let s = Stack::new("store-old-version-neutral").await;
    let other = "C:/Users/someone/Pictures/b.png";
    s.media.put_not_medium(RENAMED, H_TEXT); // the very hash the block recorded
    s.media.put(other, H3); // the block recorded H2
    s.media.put(SENTINEL, H1); // a medium, but its block recorded no hash
    let id = s.legacy_authored("Written on the phone before the update", &v2(vec![text_block(1, "now")]), "phone").await;
    let old = seed_old_version(
        &s,
        &id,
        v2(vec![
            text_block(1, "then"),
            image_block(2, RENAMED, Some(H_TEXT)),
            image_block(3, other, Some(H2)),
            image_block(4, SENTINEL, None),
        ]),
    )
    .await;
    s.svc.migrate_media_into_store().await.unwrap();
    s.media.forget_asked();
    s.svc.restore_version(&id, &old).await.unwrap();

    let sniffs: Vec<(String, usize)> = s.media.sniffed().into_iter().filter(|(p, _)| p == RENAMED).collect();
    assert!(sniffs.len() <= 1, "the non-medium's first bytes are read at most once");
    assert!(sniffs.iter().all(|(_, n)| *n <= SVG_SNIFF_BYTES), "no more than the first bytes");
    assert!(!s.media.verified().iter().any(|p| p == RENAMED), "the non-medium is never read further");
    read_at_most_once_by_the_pass(&s, other);
    assert_eq!(s.media.reads_of(SENTINEL), 0, "nothing is read for a block with no recorded hash");
    assert!(s.media.in_store().is_empty(), "nothing is copied");
    let stored = s.stored(&id).await.1;
    assert!(
        !stored.contains(RENAMED) && !stored.contains(other) && !stored.contains(SENTINEL),
        "the restored blocks are neutralised"
    );
    let mut got: Vec<(String, sovereign_docs::BlockId, String, NeutralisedReason)> = s
        .svc
        .neutralised_media()
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.document == id)
        .map(|e| (e.document, e.block, e.former_path, e.reason))
        .collect();
    got.sort_by(|a, b| a.3.cmp(&b.3));
    let mut want = vec![
        (id.clone(), bid(2), RENAMED.to_string(), NeutralisedReason::NotAMedium),
        (id.clone(), bid(3), other.to_string(), NeutralisedReason::OtherHash),
        (id.clone(), bid(4), SENTINEL.to_string(), NeutralisedReason::NoRecordedHash),
    ];
    want.sort_by(|a, b| a.3.cmp(&b.3));
    assert_eq!(got, want, "each neutralised block is recorded with its reason");
}

// v2.0.86 (Céline: « Ajouter le registre »): the notice after the pass says how many were copied and how many were not, and that the list of what was not copied is kept, encrypted, on this device; it holds no path and no title.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_the_notice_after_the_pass_counts_and_names_no_path_and_no_title() {
    const TITLE_MARKER: &str = "TITLE-MARKER-l1b-5d1a";
    const PATH_MARKER: &str = "PATH-MARKER-l1b-3e9b";
    let s = Stack::new("store-notice").await;
    let a = "C:/Users/someone/Pictures/a.png";
    let b = "C:/Users/someone/Pictures/c.png";
    let gone1 = format!("C:/Users/someone/{PATH_MARKER}/one.png");
    let gone2 = format!("C:/Users/someone/{PATH_MARKER}/two.png");
    let other = format!("C:/Users/someone/{PATH_MARKER}/other.png");
    s.media.put(a, H1);
    s.media.put(b, H2);
    s.media.put(&other, H3); // the block recorded H4
    s.legacy(
        &format!("Holiday {TITLE_MARKER}"),
        &v2(vec![
            text_block(1, "x"),
            image_block(2, a, Some(H1)),
            image_block(3, b, Some(H2)),
            image_block(4, &gone1, Some(H4)),
            image_block(5, &gone2, Some(H5)),
            image_block(6, &other, Some(H4)),
        ]),
    )
    .await;
    let run = sovereign_docs::run_media_pass_at_unlock(&s.svc).await.unwrap();
    let report = match &run {
        sovereign_docs::MediaPassRun::Ran { report, .. } => report.clone(),
        other => panic!("the pass ran on a store it had not run on: {other:?}"),
    };
    assert_eq!(report, MediaMigrationReport { copied: 2, neutralised: 3 }, "precondition: 2 copied, 3 not");
    let notice = run.notice().expect("a notice after a pass that copied and neutralised");
    let lower = notice.to_lowercase();
    assert!(notice.contains('2'), "the notice says how many were copied (2): {} chars", notice.len());
    assert!(notice.contains('3'), "the notice says how many were not copied (3): {} chars", notice.len());
    for fact in ["kept", "encrypted", "this device"] {
        assert!(lower.contains(fact), "the notice says the list of what was not copied is kept, encrypted, on this device (no « {fact} »)");
    }
    assert!(!notice.contains(PATH_MARKER), "the notice holds no path");
    assert!(!notice.contains(TITLE_MARKER), "the notice holds no title");
}

// v2.0.86, the life of a document (Céline: « Plus simple : copier partout »): an image is inserted on device A, then a word is corrected on device B, and both sync before the update, so both hold the same current state, last written on B. After the pass, A shows the image, copied and marked; B records the path as absent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_the_life_of_a_document_across_two_devices() {
    let a = Stack::with_device("store-life-a", "desktop").await;
    let b = Stack::with_device("store-life-b", "laptop").await;
    let picture = "C:/Users/someone/Pictures/inserted-on-the-desktop.png";
    a.media.put(picture, H1); // the desktop holds the image; the laptop does not
    // The state both devices hold before the update: the image inserted on the
    // desktop (its hash recorded there, G18 as built), the word corrected on
    // the laptop, synced both ways. It cannot be made through today's service
    // (which copies at insertion), so it is seeded as legacy rows, one per
    // device, last written on the laptop.
    let before = v2(vec![text_block(1, "a word corrected on the laptop"), image_block(2, picture, Some(H1))]);
    let on_a = a.legacy_authored("Holiday", &before, "laptop").await;
    let on_b = b.legacy_authored("Holiday", &before, "laptop").await;

    let ra = a.svc.migrate_media_into_store().await.unwrap();
    let rb = b.svc.migrate_media_into_store().await.unwrap();

    // The desktop: copied and marked, shown.
    assert_eq!(ra, MediaMigrationReport { copied: 1, neutralised: 0 }, "the desktop copies the image it holds");
    read_at_most_once_by_the_pass(&a, picture);
    assert_eq!(a.media.mark_of(&store_name(H1)), Some(MediaMark::CopiedByMigration), "marked « copied by the migration »");
    let img = image_of(&a.stored(&on_a).await.1);
    assert!(!img.to_string().contains(picture) && img.to_string().contains(&store_name(H1)), "the block names the store's medium");
    assert_eq!(a.svc.media_presence(&on_a).await.unwrap(), vec![(bid(2), MediaPresence::InStore)], "the desktop shows the image");

    // The laptop: the path is absent there, recorded so.
    assert_eq!(rb, MediaMigrationReport { copied: 0, neutralised: 1 });
    let record = b.svc.neutralised_media().await.unwrap();
    assert_eq!(record.len(), 1);
    assert_eq!(
        (record[0].document.as_str(), record[0].block, record[0].former_path.as_str(), record[0].reason),
        (on_b.as_str(), bid(2), picture, NeutralisedReason::Absent),
        "the laptop records the path as absent"
    );
    assert_eq!(b.svc.media_presence(&on_b).await.unwrap(), vec![(bid(2), MediaPresence::NotOnThisDevice)]);
}

/// A video block with a thumbnail, both naming outside paths with their
/// recorded hashes.
fn video_block(n: u8, path: &str, sha: &str, thumb: &str, thumb_sha: &str) -> Value {
    serde_json::json!({
        "type": "video", "id": hex(n), "ref": {"path": path, "sha256": sha}, "caption": "c",
        "thumbnail_ref": {"path": thumb, "sha256": thumb_sha}
    })
}

const MP4_HEAD: &[u8] = b"\x00\x00\x00\x18ftypisom\x00\x00\x02\x00isomiso2";
const VIDEO: &str = "C:/Users/someone/Videos/beach.mp4";
const THUMB: &str = "C:/Users/someone/Videos/beach-thumb.png";
const H_VIDEO: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const H_THUMB: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

// v2.0.86: a video's thumbnail path follows the same guards: a thumbnail that is a medium with its recorded hash is copied (and marked), like the video, and the block names neither outside path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_a_video_thumbnail_that_is_a_medium_is_copied_like_the_video() {
    let s = Stack::new("store-thumb-copy").await;
    s.media.put_with_head(VIDEO, H_VIDEO, MP4_HEAD);
    s.media.put(THUMB, H_THUMB);
    let id = s.legacy("Written here before the update", &v2(vec![text_block(1, "x"), video_block(2, VIDEO, H_VIDEO, THUMB, H_THUMB)])).await;
    let report = s.svc.migrate_media_into_store().await.unwrap();
    assert_eq!(report.neutralised, 0, "nothing neutralised");
    let mut stored = s.media.in_store();
    stored.sort();
    let mut want = vec![(store_name(H_VIDEO), H_VIDEO.to_string()), (store_name(H_THUMB), H_THUMB.to_string())];
    want.sort();
    assert_eq!(stored, want, "the video and its thumbnail are copied");
    assert_eq!(s.media.mark_of(&store_name(H_THUMB)), Some(MediaMark::CopiedByMigration), "the thumbnail is marked too");
    read_at_most_once_by_the_pass(&s, THUMB);
    let block = assert_v2(&s.stored(&id).await.1).into_iter().find(|b| b["type"] == "video").expect("the video block");
    let text = block.to_string();
    assert!(!text.contains(VIDEO) && !text.contains(THUMB), "the block names no outside path");
    assert!(text.contains(&store_name(H_THUMB)), "the block names the thumbnail's medium in the store");
}

// v2.0.86: a video's thumbnail that is not a medium is never copied, and it is recorded with its former path and its reason: never dropped without a trace.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn store_a_video_thumbnail_that_is_not_a_medium_is_recorded_never_dropped() {
    let s = Stack::new("store-thumb-record").await;
    s.media.put_with_head(VIDEO, H_VIDEO, MP4_HEAD);
    s.media.put_not_medium(THUMB, H_THUMB); // a text file named .png
    let id = s.legacy("Written here before the update", &v2(vec![text_block(1, "x"), video_block(2, VIDEO, H_VIDEO, THUMB, H_THUMB)])).await;
    s.svc.migrate_media_into_store().await.unwrap();
    assert!(!s.media.in_store().iter().any(|(_, h)| h == H_THUMB), "the thumbnail is never copied");
    assert!(!s.media.verified().iter().any(|p| p == THUMB), "it is read for its first bytes only");
    let record = s.svc.neutralised_media().await.unwrap();
    assert!(
        record.iter().any(|e| e.document == id && e.block == bid(2) && e.former_path == THUMB && e.reason == NeutralisedReason::NotAMedium),
        "the thumbnail is recorded, with its former path and its reason"
    );
    assert!(!s.stored(&id).await.1.contains(THUMB), "the block names the thumbnail's path no more");
}
