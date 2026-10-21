//! G24 — a remote state keeps its creation time and author; a missing field
//! never overwrites a known one.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G24 (specified
//! 2026-10-15, for C-WIRE): "`apply_wire` keeps what the wire carries — the
//! original creation time, the author device (the sender when none is named),
//! the stamps — and a field the sender does not send leaves the local value as
//! it is; a document new here without trust or provenance lands undetermined,
//! with provenance *paired device*."
//!
//! C-WIRE guarantee 2 (spec ch. 05): "A missing field never overwrites a known
//! one."

mod common;

use chrono::{TimeZone, Utc};
use common::wire::*;
use common::*;
use sovereign_docs::{Origin, Provenance, Trust, WireApplied, WireMeta};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g24_a_new_document_keeps_its_original_creation_time() {
    let s = Stack::new("g24-created").await;
    let created = Utc.with_ymd_and_hms(2019, 3, 4, 5, 6, 7).unwrap();
    let mut m = meta(Some(stamp(&s, 0, 0, "phone")), None);
    m.created_at = Some(created);
    let out = s.svc.apply_wire(remote_text("document:w1", "T", "x", "phone"), m).await.unwrap();
    assert!(matches!(out.applied, WireApplied::Applied(_)));
    assert_eq!(s.stored_doc("document:w1").await.created_at, created);
    assert_eq!(s.svc.stored_state("document:w1").await.unwrap().meta.created_at, Some(created));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g24_the_author_device_is_kept_apart_from_the_relaying_sender() {
    let s = Stack::new("g24-author").await;
    let mut m = meta(Some(stamp(&s, 0, 0, "phone")), None);
    m.author_device = Some("phone".into());
    s.svc.apply_wire(remote_text("document:r1", "T", "x", "laptop"), m).await.unwrap();
    assert_eq!(s.svc.stored_state("document:r1").await.unwrap().meta.author_device.as_deref(), Some("phone"));
    // Without a named author, the sender wrote it.
    s.svc
        .apply_wire(remote_text("document:r2", "T", "x", "laptop"), meta(Some(stamp(&s, 0, 0, "laptop")), None))
        .await
        .unwrap();
    assert_eq!(s.svc.stored_state("document:r2").await.unwrap().meta.author_device.as_deref(), Some("laptop"));
    // A local write is authored here.
    let id = s.create_text("Local", "x").await;
    assert_eq!(s.svc.stored_state(&id).await.unwrap().meta.author_device.as_deref(), Some(s.device.as_str()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g24_a_missing_field_never_overwrites_a_known_value() {
    let s = Stack::new("g24-missing").await;
    let created = Utc.with_ymd_and_hms(2018, 1, 2, 3, 4, 5).unwrap();
    let mut new = new_text_doc("Imported", "old text");
    new.trust = Trust::External;
    new.provenance = Provenance::Import { source_path: "D:/in/x.md".into() };
    new.origin = Origin::Import;
    new.created_at = Some(created);
    let id = s.svc.create(new).await.unwrap().id;
    let local = current_stamp(&s, &id).await;
    let mut r = remote_text(&id, "Imported", "new text", "phone");
    r.trust = None;
    r.provenance = None;
    r.trust_decision = None;
    let m = WireMeta { stamp: Some(stamp(&s, 1, 0, "phone")), prev_stamp: Some(local), ..WireMeta::default() };
    s.svc.apply_wire(r, m).await.unwrap();
    let v = s.svc.read(&id).await.unwrap();
    assert_eq!(texts(&s.stored(&id).await.1), vec!["new text"], "the content did change");
    assert_eq!(v.trust, Trust::External);
    assert_eq!(v.provenance, Provenance::Import { source_path: "D:/in/x.md".into() });
    assert_eq!(v.created_at, created);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g24_a_document_new_here_without_trust_or_provenance_gets_the_safe_defaults() {
    let s = Stack::new("g24-defaults").await;
    let mut r = remote_text("document:bare", "T", "x", "phone");
    r.trust = None;
    r.provenance = None;
    s.svc.apply_wire(r, meta(Some(stamp(&s, 0, 0, "phone")), None)).await.unwrap();
    let v = s.svc.read("document:bare").await.unwrap();
    assert_eq!(v.trust, Trust::Undetermined);
    assert_eq!(v.provenance, Provenance::Peer { device: "phone".into() });
}
