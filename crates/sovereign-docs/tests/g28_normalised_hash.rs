//! G28 — the normalised hash and the purge fingerprint.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G28 (specified
//! 2026-10-15, for C-WIRE): "The normalised hash of a state covers its title,
//! thread, trash state, trust, trust decision, provenance and its content as
//! v2 (a legacy form converted with the migration seeded by the document id):
//! equal for two copies of one document whatever form each stores, different
//! when any of these differs. The purge fingerprint covers the title and the
//! content (as v2) only. Both are lowercase hex and never contain the text."
//!
//! C-WIRE guarantee 3 (spec ch. 05): "Equal means equal after normalisation."

mod common;

use chrono::{TimeZone, Utc};
use common::wire::*;
use common::*;
use sovereign_docs::{normalized_hash, purge_fingerprint, DocumentV2, Provenance, Trust, TrustDecision};

const ENVELOPE: &str = r#"{"body":"the body","images":[{"path":"/a.png","caption":"c"}]}"#;

fn legacy_and_migrated(id: &str) -> (String, String) {
    let migrated = DocumentV2::read_seeded(ENVELOPE, id).unwrap().0.to_json_string();
    (ENVELOPE.to_string(), migrated)
}

fn hex64(h: &str) -> bool {
    h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[test]
fn g28_a_legacy_copy_and_a_migrated_copy_hash_the_same() {
    let id = "document:same";
    let (legacy, migrated) = legacy_and_migrated(id);
    let mut a = remote_text(id, "T", "x", "laptop");
    a.content = legacy;
    let mut b = a.clone();
    b.content = migrated.clone();
    assert_eq!(normalized_hash(&a), normalized_hash(&b));
    assert_eq!(purge_fingerprint(id, "T", ENVELOPE), purge_fingerprint(id, "T", &migrated));
}

#[test]
fn g28_the_sender_device_and_the_modification_time_do_not_count() {
    let a = remote_text("document:x", "T", "x", "laptop");
    let mut b = a.clone();
    b.device = "phone".into();
    b.modified_at = Utc.with_ymd_and_hms(2001, 1, 1, 0, 0, 0).unwrap();
    assert_eq!(normalized_hash(&a), normalized_hash(&b));
}

#[test]
fn g28_every_field_of_the_state_changes_the_hash() {
    let base = remote_text("document:x", "T", "x", "laptop");
    let h = normalized_hash(&base);
    let mut variants = vec![];
    let mut v = base.clone();
    v.title = "T2".into();
    variants.push(("title", v));
    let mut v = base.clone();
    v.content = v2(vec![text_block(1, "y")]);
    variants.push(("content", v));
    let mut v = base.clone();
    v.thread_id = "thread:other".into();
    variants.push(("thread", v));
    let mut v = base.clone();
    v.trust = Some(Trust::External);
    variants.push(("trust", v));
    let mut v = base.clone();
    v.trust_decision = Some(TrustDecision {
        trust: Trust::Owned,
        device: "phone".into(),
        decided_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
    });
    variants.push(("decision", v));
    let mut v = base.clone();
    v.provenance = Some(Provenance::Web { url: "https://e.org".into() });
    variants.push(("provenance", v));
    let mut v = base.clone();
    v.deleted_at = Some(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
    variants.push(("trash", v));
    for (what, v) in variants {
        assert_ne!(normalized_hash(&v), h, "{what} must change the hash");
    }
}

#[test]
fn g28_the_fingerprint_covers_title_and_content_only() {
    let id = "document:f";
    let c = v2(vec![text_block(1, "body")]);
    let f = purge_fingerprint(id, "T", &c);
    assert_ne!(purge_fingerprint(id, "T2", &c), f, "title");
    assert_ne!(purge_fingerprint(id, "T", &v2(vec![text_block(1, "other")])), f, "content");
    assert!(hex64(&f));
    // Thread, trust and trash are not parameters: they cannot change it.
}

#[test]
fn g28_hashes_are_hex_and_never_hold_the_text() {
    let mut r = remote_text("document:secret", "A secret title", "a secret body", "laptop");
    r.content = v2(vec![text_block(1, "a secret body")]);
    let h = normalized_hash(&r);
    assert!(hex64(&h));
    assert!(!h.contains("secret"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g28_the_stored_state_carries_its_own_normalised_hash() {
    let s = Stack::new("g28-stored").await;
    let id = s.create_text("T", "x").await;
    let st = s.svc.stored_state(&id).await.unwrap();
    assert_eq!(st.normalized_hash, normalized_hash(&st.document));
    // A legacy row hashes like its migrated form.
    let legacy = s.legacy("L", ENVELOPE).await;
    let before = s.svc.stored_state(&legacy).await.unwrap().normalized_hash;
    s.svc.migrate_all().await.unwrap();
    assert_eq!(s.svc.stored_state(&legacy).await.unwrap().normalized_hash, before, "migration changes no hash");
}
