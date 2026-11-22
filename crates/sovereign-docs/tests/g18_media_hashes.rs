//! G18 — media hashes are the service's.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G18: "**Media hashes
//! are the service's.** At any write, a media block without `ref.sha256` whose
//! file can be read gets the file's hash; a present hash is never changed (it
//! is what detects an altered file); an unreadable file leaves the block valid
//! without one. *As built; C-MEDIA (above) replaces the first clause in
//! v0.0.13:* a hash is computed only inside the media store, never from a path
//! that content names, and conversion hashes nothing outside the store."
//!
//! Updated for C-MEDIA (spec v2.0.79) by the test author of lot L1b: the first
//! and fourth tests assert the replaced clause; the second and third keep G18's
//! other clauses. The media store's own tests are `media_store_copy.rs`,
//! `me1_*` to `me4_*`.
//!
//! Ch. 03, *Document format v2*: "`ref.sha256` stays absent until the document
//! service computes it (the editor never reads files). Such a block is valid in
//! that state."

mod common;

use common::*;

const H: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g18_a_save_takes_the_hash_of_the_stores_copy_never_of_the_path() {
    // G18 first clause, as replaced by C-MEDIA: the user's own save of a
    // document she owns copies the medium into the store; the hash is the
    // copy's, never computed from the path the content names.
    let s = Stack::new("g18-fill").await;
    s.media.put("/m/cat.png", H);
    let id = s.create_text("T", "x").await;
    s.editor_save(&id, 1, v2(vec![text_block(1, "x"), image_block(2, "/m/cat.png", None)])).await.unwrap();
    assert!(!s.media.hashed().iter().any(|p| p == "/m/cat.png"), "no hash computed from the path content names");
    let blocks = assert_v2(&s.stored(&id).await.1);
    assert_eq!(blocks[1]["ref"]["sha256"], H, "the block carries the hash of the store's copy");
    assert!(!blocks[1].to_string().contains("/m/cat.png"), "the block names the store's medium, not the path");
    assert_eq!(blocks[1]["alt"], "a cat", "nothing else of the block changes");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g18_a_present_hash_is_never_changed() {
    let s = Stack::new("g18-keep").await;
    s.media.put("/m/cat.png", H); // the file changed since the hash was taken
    let id = s.create_text("T", "x").await;
    let old = "0000000000000000000000000000000000000000000000000000000000000000";
    s.editor_save(&id, 1, v2(vec![image_block(2, "/m/cat.png", Some(old))])).await.unwrap();
    s.assistant_save(&id, "text added").await.unwrap();
    s.svc.migrate_all().await.unwrap();
    let blocks = assert_v2(&s.stored(&id).await.1);
    let img = blocks.iter().find(|b| b["type"] == "image").unwrap();
    assert_eq!(img["ref"]["sha256"], old, "the stored hash is what detects the change");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g18_an_unreadable_file_leaves_a_valid_block_without_a_hash() {
    let s = Stack::new("g18-missing").await;
    let id = s.create_text("T", "x").await;
    s.editor_save(&id, 1, v2(vec![image_block(2, "/gone.png", None)])).await.expect("valid without a hash");
    let blocks = assert_v2(&s.stored(&id).await.1);
    assert!(blocks[0]["ref"].get("sha256").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g18_migration_hashes_nothing_outside_the_store_and_keeps_v1_media_described_as_pending() {
    // G18 as replaced by C-MEDIA: conversion hashes nothing outside the store
    // (and, *The media store*, copies nothing). v1 media stay pending a
    // description (ch. 03, *Document format v2*).
    let s = Stack::new("g18-migrate").await;
    s.media.put("/m/a.png", H);
    let id = s
        .legacy("Env", r#"{"body":"b","images":[{"path":"/m/a.png","caption":"c"},{"path":"/m/gone.png","caption":"d"}]}"#)
        .await;
    s.svc.migrate_all().await.unwrap();
    let asked = s.media.asked();
    assert!(!asked.iter().any(|p| p == "/m/a.png" || p == "/m/gone.png"), "conversion hands no path to the media files");
    let blocks = assert_v2(&s.stored(&id).await.1);
    assert!(blocks[1]["ref"].get("sha256").is_none(), "no hash from a path outside the store");
    assert!(blocks[2]["ref"].get("sha256").is_none());
    let v = s.svc.read(&id).await.unwrap();
    let doc = v.doc.as_ref().unwrap();
    assert!(doc.blocks()[1].needs_description() && doc.blocks()[2].needs_description());
}
