//! S5-12 in the native shell (`doc/plans/C-DOC-adoption.md`, *S5-12 — a live
//! document in a deleted thread asks: contract*). Written before the code, by
//! another author: whoever implements S5-12 makes these pass without editing
//! this file.
//!
//! - **W3 Never hidden** — the canvas loader draws the document in the
//!   fallback lane (`canvas::load_workspace`, no window needed).
//! - **W4 Three ways out**, as the shell offers them — through
//!   `orphans::OrphansSection` (state and service calls, no drawing).
//! - **W5 Shown where alerts are** — the section in the Alerts window
//!   (`panels::load_alerts`, its accessibility lines), and the status line's
//!   count (`orphans::status_segment`, plus a scan that `app.rs` uses it).
//! - **W6** — the peer-audit alerts beside it are unchanged.
//!
//! What is drawn, and where a click lands, is checked by hand
//! (`TRASH_CHECKLIST.md`, section 7). The service side (W1, W2, W4, W6) is
//! tested in `sovereign-docs/tests/s5_12_orphans.rs`, sync in
//! `sovereign-p2p/tests/wire_orphans.rs`.
//!
//! Stores: a real encrypted store over an in-memory SurrealDB
//! (`docs_host::testing`); nothing touches a real workspace. The other
//! device's writes arrive through the service (`apply_wire`); a thread's
//! deletion arriving from the other device is the row sync's store call
//! (`soft_delete_thread`), as in `step5_tests.rs`.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use sovereign_ai::docs_host::{build_document_service, new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use sovereign_docs::{
    delete_thread_to_trash, DocumentService, IncomingAlert, OrphanResolution, RemoteDocument, Stamp, Trust, WireMeta,
};

use crate::orphans::{status_segment, OrphanAction, OrphansSection, ACTIONS, HEADING};

/// Text that must never appear in the section.
const CONTENT_MARKER: &str = "zebra-orphan-marker-12";
const PHONE: &str = "peer-phone-12D3";

async fn fixture(tag: &str) -> (Arc<EncryptedGraphDB>, DocumentService) {
    let store = testing::encrypted_memory_store(tag).await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), "desktop".into());
    (store, docs)
}

async fn thread(store: &EncryptedGraphDB, name: &str) -> String {
    store.create_thread(Thread::new(name.into(), String::new())).await.unwrap().id_string().unwrap()
}

async fn create(docs: &DocumentService, title: &str, thread_id: &str, text: &str) -> String {
    docs.create(new_user_document(title, thread_id, text)).await.unwrap().id
}

fn names() -> HashMap<String, String> {
    HashMap::from([(PHONE.to_string(), "Phone".to_string())])
}

/// A document written on the phone, for `thread_id`, arriving by sync.
async fn arrive(docs: &DocumentService, id: &str, thread_id: &str, title: &str) {
    let content = serde_json::json!({"format": "sovereign.doc", "v": 2,
        "blocks": [{"type": "text", "id": "00000000000000000000000000001001", "markup": "markdown", "text": CONTENT_MARKER}]})
    .to_string();
    let remote = RemoteDocument {
        id: id.into(),
        title: title.into(),
        content,
        thread_id: thread_id.into(),
        trust: Some(Trust::Owned),
        provenance: None,
        modified_at: Utc::now(),
        deleted_at: None,
        device: PHONE.into(),
        trust_decision: None,
    };
    let meta = WireMeta {
        stamp: Some(Stamp { wall_ms: Utc::now().timestamp_millis(), counter: 0, device: PHONE.into() }),
        ..WireMeta::default()
    };
    docs.apply_wire(remote, meta).await.expect("applied");
}

/// "Old drafts" deleted here; two documents of it written on the phone arrive
/// (`a`, `b`). "Notes" deleted on the phone; this device's own document of it
/// (`mine`) is live. "Keep" stays, with `kept`.
struct World {
    store: Arc<EncryptedGraphDB>,
    docs: DocumentService,
    drafts: String,
    notes: String,
    keep: String,
    a: String,
    b: String,
    mine: String,
    kept: String,
}

async fn world(tag: &str) -> World {
    let (store, docs) = fixture(tag).await;
    let drafts = thread(&store, "Old drafts").await;
    let notes = thread(&store, "Notes").await;
    let keep = thread(&store, "Keep").await;
    let old = create(&docs, "Old", &drafts, "old").await;
    let mine = create(&docs, "Mine", &notes, CONTENT_MARKER).await;
    let kept = create(&docs, "Kept", &keep, "kept").await;
    delete_thread_to_trash(&docs, store.as_ref(), &drafts).await.expect("deleted here");
    assert!(docs.read(&old).await.unwrap().trashed_at.is_some());
    // Alphanumeric keys, as a peer's store generates them (a key with a hyphen
    // is displayed `document:⟨…⟩` on the canvas card).
    let (a, b) = ("document:phonea1".to_string(), "document:phoneb1".to_string());
    arrive(&docs, &a, &drafts, "Alpha from the phone").await;
    arrive(&docs, &b, &drafts, "Beta from the phone").await;
    store.soft_delete_thread(&notes).await.unwrap();
    World { store, docs, drafts, notes, keep, a, b, mine, kept }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

async fn section(w: &World) -> OrphansSection {
    OrphansSection::load(&w.docs, &names()).await.expect("the section loads")
}

fn ids(s: &OrphansSection) -> Vec<String> {
    sorted(s.lines().iter().map(|l| l.id.clone()).collect())
}

// -------------------------------------------------------------------- W3 --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w3_the_canvas_draws_a_waiting_document_in_the_fallback_lane() {
    let w = world("w3-canvas").await;
    let db: Arc<dyn GraphDB> = w.store.clone();
    let mut shaper = crate::text::TextShaper::new();
    let (cards, _links, _w, lane_names, _ref) =
        crate::canvas::load_workspace(&db, &mut shaper).await.expect("the workspace loads");
    for id in [&w.a, &w.b, &w.mine] {
        let card = cards.iter().find(|c| &c.id == id).unwrap_or_else(|| panic!("{id} is on the canvas"));
        assert_eq!(lane_names[card.lane], crate::canvas::FALLBACK_LANE_NAME, "{id} in the fallback lane");
    }
    let kept = cards.iter().find(|c| c.id == w.kept).expect("kept on the canvas");
    assert_eq!(lane_names[kept.lane], "Keep");
    for gone in ["Old drafts", "Notes"] {
        assert!(!lane_names.iter().any(|n| n == gone), "no lane for the deleted thread {gone}");
    }
}

// -------------------------------------------------------------------- W5 --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w5_the_section_lists_each_waiting_document_with_its_old_thread_and_no_text() {
    let w = world("w5-lines").await;
    let s = section(&w).await;
    assert_eq!(ids(&s), sorted(vec![w.a.clone(), w.b.clone(), w.mine.clone()]));
    assert_eq!(s.count(), 3);
    let a = s.lines().iter().find(|l| l.id == w.a).unwrap();
    assert_eq!(a.title, "Alpha from the phone");
    assert_eq!(a.thread_id, w.drafts);
    assert_eq!(a.old_thread_name, "Old drafts");
    assert_eq!(a.device, "Phone", "a paired device by its name");
    let mine = s.lines().iter().find(|l| l.id == w.mine).unwrap();
    assert_eq!(mine.old_thread_name, "Notes");
    for l in s.lines() {
        let n = l.notice();
        assert!(n.contains(&l.title), "the notice names the document: {n}");
        assert!(n.contains(&l.old_thread_name), "the notice names the deleted thread: {n}");
        assert!(n.to_lowercase().contains("deleted"), "the notice says the thread was deleted: {n}");
        assert!(!n.contains(CONTENT_MARKER), "no document text: {n}");
    }
    let all = s.a11y_lines().join("\n");
    assert!(!all.contains(CONTENT_MARKER), "no document text in the accessibility lines");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w5_every_line_offers_the_three_ways_out_and_each_deleted_thread_its_group_action() {
    let w = world("w5-actions").await;
    let s = section(&w).await;
    assert_eq!(ACTIONS, [OrphanAction::MoveTo, OrphanAction::NewThread, OrphanAction::Trash]);
    let groups = s.groups();
    let drafts = groups.iter().find(|g| g.thread_id == w.drafts).expect("a group for Old drafts");
    assert_eq!(sorted(drafts.ids.clone()), sorted(vec![w.a.clone(), w.b.clone()]));
    assert!(drafts.label.contains("Old drafts"), "the group action names the deleted thread: {}", drafts.label);
    let notes = groups.iter().find(|g| g.thread_id == w.notes).expect("a group for Notes");
    assert_eq!(notes.ids, vec![w.mine.clone()]);
    assert_eq!(groups.len(), 2);

    let lines = s.a11y_lines();
    assert_eq!(lines.first().map(String::as_str), Some(HEADING), "the section's heading first");
    for l in s.lines() {
        let said = lines.iter().find(|x| x.contains(&l.notice())).unwrap_or_else(|| panic!("{} is read out", l.id));
        let low = said.to_lowercase();
        for action in ["move", "new thread", "trash"] {
            assert!(low.contains(action), "the line says its action {action:?}: {said}");
        }
    }
    assert!(lines.iter().any(|x| x.contains(&drafts.label)), "the group action is read out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w5_the_alerts_window_carries_the_section_beside_the_peer_alerts() {
    let w = world("w5-window").await;
    w.docs
        .raise_alert(IncomingAlert {
            id: w.kept.clone(),
            device: PHONE.into(),
            finding: "injected instructions in the new content".into(),
            previous_version: None,
        })
        .await
        .unwrap();
    let panel = crate::panels::load_alerts(&w.docs, &names()).await;
    // W6: the peer alert is there, as before, and only it is an alert row.
    assert_eq!(panel.rows.len(), 1, "the section adds no alert row");
    assert_eq!(panel.rows[0].doc_id, w.kept);
    // W5: the section is in the same window.
    assert_eq!(panel.orphans().count(), 3);
    let lines = panel.a11y_lines();
    assert!(lines.iter().any(|l| l.starts_with("Alert 1 of 1")), "the peer alert is read out as before");
    assert!(lines.iter().any(|l| l == HEADING), "the section's heading is read out in the Alerts window");
    for l in panel.orphans().lines() {
        assert!(lines.iter().any(|x| x.contains(&l.notice())), "{} is read out in the Alerts window", l.id);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w5_with_no_peer_alert_the_window_still_shows_the_section() {
    let w = world("w5-only").await;
    let panel = crate::panels::load_alerts(&w.docs, &names()).await;
    assert!(panel.rows.is_empty());
    assert_eq!(panel.orphans().count(), 3);
    assert!(panel.a11y_lines().iter().any(|l| l == HEADING));
}

#[test]
fn w5_the_status_line_counts_the_waiting_documents() {
    assert_eq!(status_segment(0), None, "nothing said when nothing waits");
    for n in [1usize, 2, 17] {
        let s = status_segment(n).unwrap_or_else(|| panic!("{n} waiting: the status line says so"));
        assert!(s.contains(&n.to_string()), "names the count {n}: {s}");
    }
}

/// `App` cannot be built without a window: this reads `app.rs` for a call to
/// `status_segment` in production code (the status line's text).
#[test]
fn w5_the_status_line_uses_the_count() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/app.rs")).unwrap();
    let prod = match src.find("#[cfg(test)]\nmod taskbar_tests") {
        Some(at) => &src[..at],
        None => &src[..],
    };
    let used = prod.lines().map(|l| l.split("//").next().unwrap_or("")).any(|code| code.contains("status_segment("));
    assert!(used, "app.rs never puts the waiting documents' count in the status line");
}

// ----------------------------------------------------- W4, as the shell does --

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_each_way_out_ends_the_question_at_once() {
    let w = world("w4-shell").await;
    let mut s = section(&w).await;

    s.resolve(&w.docs, &w.a, OrphanResolution::MoveTo(w.keep.clone())).await.expect("moved");
    assert!(!ids(&s).contains(&w.a), "dropped at once");
    assert_eq!(w.docs.read(&w.a).await.unwrap().thread_id, w.keep);

    s.resolve(&w.docs, &w.mine, OrphanResolution::NewThread("Notes again".into())).await.expect("new thread");
    assert!(!ids(&s).contains(&w.mine));
    let t = w.docs.read(&w.mine).await.unwrap().thread_id;
    assert_eq!(w.store.get_thread(&t).await.unwrap().name, "Notes again");

    s.resolve(&w.docs, &w.b, OrphanResolution::Trash).await.expect("trashed");
    assert!(w.docs.read(&w.b).await.unwrap().trashed_at.is_some());
    assert_eq!(s.count(), 0);
    assert!(w.docs.orphans_of_deleted_threads().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_a_refused_way_out_says_why_and_changes_nothing() {
    let w = world("w4-shell-refused").await;
    let mut s = section(&w).await;
    for (what, r) in [
        ("a deleted thread", OrphanResolution::MoveTo(w.notes.clone())),
        ("an unknown thread", OrphanResolution::MoveTo("thread:nope".into())),
        ("an empty name", OrphanResolution::NewThread("  ".into())),
    ] {
        let msg = s.resolve(&w.docs, &w.a, r).await.expect_err(what);
        assert!(!msg.trim().is_empty(), "{what}: the notice says why");
        assert!(ids(&s).contains(&w.a), "{what}: still listed");
        assert_eq!(w.docs.read(&w.a).await.unwrap().thread_id, w.drafts, "{what}: not moved");
    }
    assert_eq!(s.count(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_the_group_action_resolves_all_of_one_deleted_thread() {
    let w = world("w4-shell-group").await;
    let mut s = section(&w).await;
    let n = s.resolve_group(&w.docs, &w.drafts, OrphanResolution::NewThread("Drafts".into())).await.expect("group");
    assert_eq!(n, 2);
    let (ta, tb) = (w.docs.read(&w.a).await.unwrap().thread_id, w.docs.read(&w.b).await.unwrap().thread_id);
    assert_eq!(ta, tb, "one new thread for the group");
    assert_eq!(w.store.get_thread(&ta).await.unwrap().name, "Drafts");
    assert_eq!(ids(&s), vec![w.mine.clone()], "the other deleted thread's document still waits");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w4_a_document_resolved_elsewhere_meanwhile_is_refused_and_dropped() {
    // The section was loaded; then the document was moved by sync (here,
    // through the service, as sync would).
    let w = world("w4-shell-stale").await;
    let mut s = section(&w).await;
    w.docs.resolve_orphan(&w.a, OrphanResolution::MoveTo(w.keep.clone())).await.unwrap();
    let msg = s.resolve(&w.docs, &w.a, OrphanResolution::Trash).await.expect_err("no longer waiting");
    assert!(!msg.trim().is_empty());
    assert!(w.docs.read(&w.a).await.unwrap().trashed_at.is_none(), "not trashed by a stale click");
    assert_eq!(w.docs.read(&w.a).await.unwrap().thread_id, w.keep);
    assert!(!ids(&s).contains(&w.a), "the section shows what is true now");
}
