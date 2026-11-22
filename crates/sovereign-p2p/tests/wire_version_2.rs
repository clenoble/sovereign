//! C-WIRE guarantee 12 as of spec v2.0.86 — the wire version goes up with the
//! document format; and C-MEDIA's record never travels by row sync or backup.
//!
//! Spec ch. 05, *Peers on an older build* (Céline, 2026-11-14: « Oui,
//! l'appliquer »): "A peer is on an older build when its manifest carries a
//! lower wire version, or none. [...] v0.0.13 raises it from 1 to 2 [...] The
//! refusals are the new build's. A build that only tests for a missing version,
//! as v0.0.12 does, does not compare numbers. So toward a peer on a lower wire
//! version, the new build: sends it no document state; serves none of its
//! requests for documents; applies none of its document states." Threads and
//! the other rows keep syncing.
//!
//! A v0.0.12 peer is simulated at its manifest: it announces wire version 1
//! and, unlike the builds before v0.0.11, carries its document entries and its
//! envelopes carry their wire state. Both starting orders are covered: this
//! build starting the round (its plan), and the older peer starting it (its
//! requests served, its pushes applied, here). The screens' sentence already
//! has its tests (`frontend/src/lib/stores/syncAlerts.svelte.test.ts`, the
//! shell's status bar); what fires it is `plan.blocked_old_peer`, asserted
//! here.
//!
//! As amended (v2.0.86, main 5e6e201): "Its purge markers are still applied"
//! — they travel in the manifest, while a soft deletion travels inside a
//! document state and is refused with it; and "The rule holds from both
//! sides. A build that meets a higher wire version holds the same three
//! refusals. Its screen says that this device runs the older version and is
//! the one to update." The plan says which side is older
//! (`blocked_old_peer`, `this_device_older`).
//!
//! v2.0.91 (Fable 0041, laptop 0264, 0265; the announcement of v2.0.90
//! withdrawn): "The answering side judges by the request it answers, whichever
//! side started the round. The two requests that carry documents [...] carry
//! the requester's wire version. A request built by v0.0.12 has no such field,
//! and reads as having none. A request with no version, or a lower one, is
//! refused for documents [...] A request with a higher version is refused the
//! same way, and this device knows it is the older one. The refusal is said to
//! the requester [...] Nothing is remembered per peer on the answering side
//! [...] Any later request that carries a document joins this rule. A test
//! lists them, and a new one fails until it is named." Toward no version or a
//! lower one, the refusal is `SovereignResponse::Error { message }`, which
//! v0.0.12 decodes.
//!
//! **No test tells the answering side who the peer is.** It is driven through
//! the node's own request handler (`sovereign_p2p::node::process_request`) with
//! the requests the other side sends; a v0.0.12 request is built from its own
//! enum's shape and encoded by the protocol's CBOR codec. The asking side's plan
//! (`blocked_old_peer`, `this_device_older`) stays tested above, from the
//! manifest it asked for.
//!
//! Spec ch. 03, *The media store*, v2.0.86: the record of what the pass
//! neutralised "is never synced or backed up by the database either".

mod wire_common;

use sovereign_db::GraphDB;
use sovereign_p2p::protocol::manifest::SyncManifest;
use sovereign_p2p::protocol::sync::SyncTable;
use sovereign_p2p::protocol::{SovereignRequest, SovereignResponse};
use sovereign_p2p::P2pEvent;
use sovereign_p2p::sync_engine::compute_all_row_diffs;
use sovereign_p2p::wire::{plan_documents, WIRE_VERSION};
use wire_common::*;

/// The manifest a v0.0.12 peer sends: wire version 1, its documents listed.
fn as_wire_version_1(mut m: SyncManifest) -> SyncManifest {
    m.wire_version = Some(1);
    m
}

/// Two paired devices, each with a document of its own, and a thread on `b`.
async fn two_devices(tag: &str) -> (Device, Device, String, String) {
    let (a, b) = (device(&format!("{tag}-a"), 41).await, device(&format!("{tag}-b"), 42).await);
    pair(&a, &b);
    let on_a = create(&a, "Only on A", "a").await;
    let on_b = create(&b, "Only on B", "b").await;
    b.edb.create_thread(sovereign_db::schema::Thread::new("Lane on B".into(), String::new())).await.unwrap();
    (a, b, on_a, on_b)
}

// C-WIRE 12 (v2.0.86): this build speaks wire version 2.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire12_this_build_speaks_wire_version_2() {
    assert_eq!(WIRE_VERSION, 2, "v0.0.13 raises the wire version with the document format");
    let a = device("w12v2-speaks", 40).await;
    assert_eq!(manifest(&a).await.wire_version, Some(2), "and its manifest says so");
}

// C-WIRE 12 (v2.0.86), this build starting the round: a peer announcing wire version 1 is an older build; no document is fetched from it or sent to it, and rows still sync.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire12_v2_starting_a_round_with_a_v1_peer_exchanges_no_document() {
    let (a, b, _on_a, _on_b) = two_devices("w12v2-start").await;
    let ma = manifest(&a).await;
    let old = as_wire_version_1(manifest(&b).await);
    assert!(!old.document_wire.is_empty(), "precondition: the v1 peer lists its documents, as v0.0.12 does");
    let plan = plan_documents(&ma, &old);
    assert!(plan.blocked_old_peer, "a lower wire version is an older build (and the screens say which device to update)");
    assert!(!plan.this_device_older, "this device is not the older one: the peer is");
    assert!(plan.fetch.is_empty() && plan.push.is_empty() && plan.concurrent.is_empty(), "no document exchanged: {plan:?}");
    assert!(compute_all_row_diffs(&ma, &old).get(&SyncTable::Thread).is_some_and(|d| d.has_work()), "rows still sync with it");
}

// C-WIRE 12 (v2.0.86, as amended): "Its purge markers are still applied. A marker carries an id and a stamp, and no document content [...] A purge is the user's intent." A purge marker from a v1 peer's manifest is applied here.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire12_v2_applies_the_purge_markers_of_a_v1_peer() {
    let (a, b) = (device("w12v2-del-a", 43).await, device("w12v2-del-b", 44).await);
    pair(&a, &b);
    let shared = create(&a, "Shared", "s").await;
    settle(&a, &b, 3).await; // both on wire version 2 so far: the document is on both
    assert!(b.edb.get_document(&shared).await.is_ok(), "precondition: the document reached b");
    b.docs.trash(&shared).await.unwrap();
    b.docs.purge_from_trash(&shared).await.unwrap();
    let old = as_wire_version_1(manifest(&b).await);
    assert!(!old.purge_markers.is_empty(), "precondition: the peer's manifest carries the deletion");
    let changed = a.sync.apply_purge_markers(&old).await.expect("the markers are read");
    assert!(changed >= 1, "the purge marker of a peer on wire version 1 is applied");
    let d = a.edb.get_document(&shared).await.expect("a received purge only trashes here");
    assert!(d.deleted_at.is_some(), "the document went to this device's trash (G27)");
}

/// A string no real row holds by chance.
const MARKER: &str = "MARKER-l1b-p2p-6a0f";

// C-MEDIA (v2.0.86): the record of what the pass neutralised is never carried by the row sync or by a backup: a device that receives every row has no record, and the backup snapshot holds no recorded path.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn media_record_travels_neither_by_row_sync_nor_by_backup() {
    let (a, b) = (device("rec-rows-a", 45).await, device("rec-rows-b", 46).await);
    pair(&a, &b);
    let former = format!("C:/Users/someone/{MARKER}/holiday.png");
    let content = serde_json::json!({"format": "sovereign.doc", "v": 2, "blocks": [
        {"type": "text", "id": format!("{:032x}", 1), "markup": "markdown", "text": "x"},
        {"type": "image", "id": format!("{:032x}", 2), "ref": {"path": former, "sha256": "ab".repeat(32)}, "alt": "a picture", "caption": ""}
    ]});
    let mut d = sovereign_db::schema::Document::new("Before the update".into(), "thread:t1".into(), true);
    d.content = content.to_string();
    a.edb.create_document(d).await.expect("a row from before the update");
    a.docs.migrate_media_into_store().await.expect("the pass");
    assert!(
        a.docs.neutralised_media().await.unwrap().iter().any(|e| e.former_path == former),
        "precondition: the record holds the former path"
    );

    // Every row a can send to b, for every table.
    let (ma, mb) = (manifest(&a).await, manifest(&b).await);
    for (table, diff) in compute_all_row_diffs(&ma, &mb) {
        if diff.push_to_remote.is_empty() {
            continue;
        }
        let rows = a.sync.get_rows(table, &diff.push_to_remote, &b.peer).await.expect("rows from a");
        let _ = b.sync.apply_rows(table, rows, &a.peer).await;
    }
    assert!(b.docs.neutralised_media().await.unwrap().is_empty(), "the record travelled by row sync");

    // The backup: the snapshot is plaintext; the record's path is not in it.
    let snapshot = sovereign_p2p::backup::build_snapshot(a.edb.as_ref(), "rec-rows-a").await.expect("snapshot");
    let json = serde_json::to_string(&snapshot).expect("the snapshot serialises");
    assert!(!json.contains(MARKER), "the backup carries a recorded path");
}

/// The manifest a later build sends: wire version 3, against this build's 2.
fn as_wire_version_3(mut m: SyncManifest) -> SyncManifest {
    m.wire_version = Some(3);
    m
}

// C-WIRE 12 from the other side (v2.0.86), this build starting the round: a peer announcing wire version 3 makes this device the older one; no document is fetched or sent, rows still sync, and the plan says this device is the one to update.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire12_v2_starting_a_round_with_a_v3_peer_exchanges_no_document_and_knows_it_is_older() {
    let (a, b, _on_a, _on_b) = two_devices("w12v3-start").await;
    let ma = manifest(&a).await;
    let newer = as_wire_version_3(manifest(&b).await);
    let plan = plan_documents(&ma, &newer);
    assert!(plan.fetch.is_empty() && plan.push.is_empty() && plan.concurrent.is_empty(), "no document exchanged: {plan:?}");
    assert!(plan.this_device_older, "the plan says this device runs the older version (its screen says to update it)");
    assert!(!plan.blocked_old_peer, "the peer is not the older one");
    assert!(compute_all_row_diffs(&ma, &newer).get(&SyncTable::Thread).is_some_and(|d| d.has_work()), "rows still sync with it");
}

// ------------------------------------------- v2.0.91, the answering side --

/// One request from `from`, handled at `at` by the node's own request handler;
/// returns its answer and the events it raised.
async fn ask(at: &Device, from: &Device, request: SovereignRequest) -> (SovereignResponse, Vec<P2pEvent>) {
    let (events, mut rx) = tokio::sync::mpsc::channel(64);
    let response = sovereign_p2p::node::process_request(request, from.peer, &events, &at.sync, None).await;
    drop(events);
    let mut raised = vec![];
    while let Ok(e) = rx.try_recv() {
        raised.push(e);
    }
    (response, raised)
}

/// A request as v0.0.12 builds it: its own enum's shape, with no version
/// field, encoded by the protocol's codec (CBOR), and read back by this build.
mod v0_0_12 {
    use serde::{Deserialize, Serialize};
    use sovereign_p2p::protocol::sync::EncryptedCommit;

    #[derive(Serialize, Deserialize)]
    pub enum SovereignRequest {
        GetCommits { commit_ids: Vec<String> },
        PushCommits { commits: Vec<EncryptedCommit> },
    }
}

async fn as_v0_0_12_sends_it(old: v0_0_12::SovereignRequest) -> SovereignRequest {
    use libp2p::futures::io::Cursor;
    use libp2p::request_response::Codec;
    let protocol = libp2p::StreamProtocol::new("/sovereign/sync/1.0.0");
    let mut wire = Cursor::new(Vec::new());
    let mut old_codec = libp2p::request_response::cbor::codec::Codec::<v0_0_12::SovereignRequest, SovereignResponse>::default();
    old_codec.write_request(&protocol, &mut wire, old).await.expect("v0.0.12 encodes its request");
    let mut read = Cursor::new(wire.into_inner());
    let mut codec = libp2p::request_response::cbor::codec::Codec::<SovereignRequest, SovereignResponse>::default();
    codec.read_request(&protocol, &mut read).await.expect("this build decodes a v0.0.12 request")
}

fn get_commits(ids: &[&str], version: Option<u32>) -> SovereignRequest {
    SovereignRequest::GetCommits { commit_ids: ids.iter().map(|s| s.to_string()).collect(), wire_version: version }
}

/// The kind of an answer, for messages (a document list is long).
fn kind(r: &SovereignResponse) -> String {
    match r {
        SovereignResponse::Commits { commits } => format!("Commits ({} documents)", commits.len()),
        SovereignResponse::Error { message } => format!("Error ({message})"),
        SovereignResponse::Ok => "Ok".into(),
        _ => "another answer".into(),
    }
}

/// The refusal is said, not answered as « nothing to sync »: an error with a
/// reason, the variant v0.0.12 decodes.
fn refused_with_an_error(r: &SovereignResponse) -> bool {
    matches!(r, SovereignResponse::Error { message } if !message.is_empty())
}

/// A refusal of any form (for a higher version the form is the implementer's):
/// neither a list of documents nor a plain acknowledgement.
fn refused_said(r: &SovereignResponse) -> bool {
    !matches!(r, SovereignResponse::Commits { .. } | SovereignResponse::Ok)
}

/// `from` fetches `at_doc` and pushes `from_doc` with `version` (`None`: built
/// as v0.0.12 builds it), then pushes its thread rows. Returns the two
/// answers, whether `from_doc` landed, and whether the thread did.
async fn exchange(at: &Device, from: &Device, at_doc: &str, from_doc: &str, version: Option<u32>) -> (SovereignResponse, SovereignResponse, bool, bool, Vec<P2pEvent>) {
    let env = from.sync.get_commits_since(from_doc, None, &at.peer).await.expect("seal its own state");
    let (fetch, push) = match version {
        None => (
            as_v0_0_12_sends_it(v0_0_12::SovereignRequest::GetCommits { commit_ids: vec![at_doc.to_string()] }).await,
            as_v0_0_12_sends_it(v0_0_12::SovereignRequest::PushCommits { commits: env }).await,
        ),
        Some(_) => (get_commits(&[at_doc], version), SovereignRequest::PushCommits { commits: env, wire_version: version }),
    };
    let (served, mut events) = ask(at, from, fetch).await;
    let (pushed, more) = ask(at, from, push).await;
    events.extend(more);
    let applied = at.edb.get_document(from_doc).await.is_ok();
    let (m_from, m_at) = (manifest(from).await, manifest(at).await);
    let ids = compute_all_row_diffs(&m_from, &m_at).remove(&SyncTable::Thread).map(|d| d.push_to_remote).unwrap_or_default();
    if !ids.is_empty() {
        let rows = from.sync.get_rows(SyncTable::Thread, &ids, &at.peer).await.expect("its thread rows");
        let _ = ask(at, from, SovereignRequest::PushRows { table: SyncTable::Thread, rows }).await;
    }
    let thread = at.edb.list_threads().await.unwrap_or_default().iter().any(|t| t.name == "Lane on B");
    (served, pushed, applied, thread, events)
}

// C-WIRE 12 (v2.0.91): a GetCommits and a PushCommits encoded as v0.0.12 encodes them (its enum's shape, no version field, the protocol's CBOR codec) decode here with the version empty.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire12_document_requests_built_by_v0_0_12_decode_with_no_version() {
    match as_v0_0_12_sends_it(v0_0_12::SovereignRequest::GetCommits { commit_ids: vec!["document:x".into()] }).await {
        SovereignRequest::GetCommits { commit_ids, wire_version } => {
            assert_eq!(commit_ids, vec!["document:x".to_string()]);
            assert_eq!(wire_version, None, "a v0.0.12 fetch carries no version");
        }
        other => panic!("decoded as another request: {other:?}"),
    }
    match as_v0_0_12_sends_it(v0_0_12::SovereignRequest::PushCommits { commits: vec![] }).await {
        SovereignRequest::PushCommits { wire_version, .. } => assert_eq!(wire_version, None, "a v0.0.12 push carries no version"),
        other => panic!("decoded as another request: {other:?}"),
    }
}

// C-WIRE 12 (v2.0.91): a request with no version (built by v0.0.12), whichever side started the round, is refused for documents with an Error: nothing served, nothing applied; rows still sync.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire12_a_request_with_no_version_is_refused_for_documents_and_rows_still_sync() {
    let (a, b, on_a, on_b) = two_devices("w12v91-none").await;
    let (served, pushed, applied, thread, _) = exchange(&a, &b, &on_a, &on_b, None).await;
    assert!(refused_with_an_error(&served), "a fetch with no version is refused with an Error (as v0.0.12 decodes it): {}", kind(&served));
    assert!(refused_with_an_error(&pushed), "a push with no version is refused with an Error: {}", kind(&pushed));
    assert!(!applied, "a document state from a request with no version was applied");
    assert!(thread, "rows still sync");
}

// C-WIRE 12 (v2.0.91): a request with a lower version is refused for documents the same way, with an Error; rows still sync.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire12_a_request_with_a_lower_version_is_refused_for_documents_and_rows_still_sync() {
    let (a, b, on_a, on_b) = two_devices("w12v91-lower").await;
    let (served, pushed, applied, thread, _) = exchange(&a, &b, &on_a, &on_b, Some(WIRE_VERSION - 1)).await;
    assert!(refused_with_an_error(&served), "a fetch with a lower version is refused with an Error: {}", kind(&served));
    assert!(refused_with_an_error(&pushed), "a push with a lower version is refused with an Error: {}", kind(&pushed));
    assert!(!applied, "a document state from a request with a lower version was applied");
    assert!(thread, "rows still sync");
}

// C-WIRE 12 (v2.0.91): a request with a higher version is refused for documents, the refusal is said, and this device knows it is the older one; rows still sync.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire12_a_request_with_a_higher_version_is_refused_and_this_device_knows_it_is_older() {
    let (a, b, on_a, on_b) = two_devices("w12v91-higher").await;
    let (served, pushed, applied, thread, events) = exchange(&a, &b, &on_a, &on_b, Some(WIRE_VERSION + 1)).await;
    assert!(refused_said(&served), "a fetch with a higher version is served: {}", kind(&served));
    assert!(refused_said(&pushed), "a push with a higher version is acknowledged: {}", kind(&pushed));
    assert!(!applied, "a document state from a request with a higher version was applied");
    assert!(
        events.iter().any(|e| matches!(e, P2pEvent::ThisDeviceNeedsUpdate { peer_id } if *peer_id == b.peer.to_string())),
        "this device says it is the one to update"
    );
    assert!(thread, "rows still sync");
}

// C-WIRE 12 (v2.0.91): a request with this build's version is served and applied.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire12_a_request_with_this_builds_version_is_served_and_applied() {
    let (a, b, on_a, on_b) = two_devices("w12v91-equal").await;
    let (served, pushed, applied, thread, _) = exchange(&a, &b, &on_a, &on_b, Some(WIRE_VERSION)).await;
    assert!(matches!(&served, SovereignResponse::Commits { commits } if !commits.is_empty()), "served: {}", kind(&served));
    assert!(matches!(pushed, SovereignResponse::Ok), "the push is acknowledged: {}", kind(&pushed));
    assert!(applied, "its document state is applied");
    assert!(thread, "rows sync");
}

// C-WIRE 12 (v2.0.91): nothing is remembered per peer on the answering side; a device put back on v0.0.12 after syncing at this build's version is refused at its first request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wire12_a_device_put_back_on_v0_0_12_is_refused_at_its_first_request() {
    let (a, b, on_a, on_b) = two_devices("w12v91-back").await;
    let (served, _, applied, _, _) = exchange(&a, &b, &on_a, &on_b, Some(WIRE_VERSION)).await;
    assert!(matches!(&served, SovereignResponse::Commits { commits } if !commits.is_empty()), "precondition: at this build's version, documents sync");
    assert!(applied, "precondition: its state was applied");
    // Put back on v0.0.12, in the same process: its requests carry no version.
    let newer_on_b = create(&b, "Written on b after going back", "c").await;
    let (served, pushed, applied, _, _) = exchange(&a, &b, &on_a, &newer_on_b, None).await;
    assert!(refused_with_an_error(&served), "its first fetch, with no version, is refused: {}", kind(&served));
    assert!(refused_with_an_error(&pushed), "its first push, with no version, is refused: {}", kind(&pushed));
    assert!(!applied, "its new document was applied");
}

// C-WIRE 12 (v2.0.91): "Any later request that carries a document joins this rule. A test lists them, and a new one fails until it is named." The list is this exhaustive match: a new variant does not compile here until it is named; and the code's own list agrees with it.
#[test]
fn wire12_the_requests_that_carry_documents_are_named() {
    fn carries_documents(r: &SovereignRequest) -> bool {
        match r {
            SovereignRequest::GetCommits { .. } | SovereignRequest::PushCommits { .. } => true,
            SovereignRequest::GetManifest
            | SovereignRequest::PushManifest(_)
            | SovereignRequest::GetRows { .. }
            | SovereignRequest::PushRows { .. }
            | SovereignRequest::DeliverShard(_)
            | SovereignRequest::RequestShard(_)
            | SovereignRequest::StoreBackupFragment { .. }
            | SovereignRequest::ListBackups { .. }
            | SovereignRequest::FetchBackupFragment { .. }
            | SovereignRequest::PairHello { .. }
            | SovereignRequest::PairProof { .. }
            | SovereignRequest::PairComplete { .. }
            | SovereignRequest::GuardianHello { .. }
            | SovereignRequest::GuardianProof { .. }
            | SovereignRequest::GuardianComplete { .. } => false,
        }
    }
    let samples = vec![
        get_commits(&["document:x"], None),
        SovereignRequest::PushCommits { commits: vec![], wire_version: None },
        SovereignRequest::GetManifest,
        SovereignRequest::GetRows { table: SyncTable::Thread, ids: vec![] },
        SovereignRequest::PushRows { table: SyncTable::Thread, rows: vec![] },
        SovereignRequest::ListBackups { owner_tag: None },
        SovereignRequest::FetchBackupFragment { owner_tag: "o".into(), snapshot_id: "s".into(), index: 0 },
        SovereignRequest::PairHello { offer_id: "o".into(), device_name: "d".into() },
        SovereignRequest::GuardianProof { offer_id: "o".into(), proof: vec![] },
    ];
    for r in &samples {
        assert_eq!(r.carries_documents(), carries_documents(r), "the code and the list disagree on {r:?}");
    }
}

// C-WIRE 12 (v2.0.91): this build's requests for documents carry its wire version.
#[test]
fn wire12_this_builds_document_requests_carry_its_version() {
    match SovereignRequest::get_commits(vec!["document:x".into()]) {
        SovereignRequest::GetCommits { wire_version, .. } => assert_eq!(wire_version, Some(WIRE_VERSION)),
        other => panic!("not a fetch: {other:?}"),
    }
    match SovereignRequest::push_commits(vec![]) {
        SovereignRequest::PushCommits { wire_version, .. } => assert_eq!(wire_version, Some(WIRE_VERSION)),
        other => panic!("not a push: {other:?}"),
    }
}
