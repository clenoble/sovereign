//! End-to-end sync test for the v0.0.5 protocol layer (plan §6.2).
//!
//! Spins up two `SovereignNode` instances on `127.0.0.1` with random
//! ports, dials them together (no mDNS reliance — Windows + WSL +
//! random CI environments make multicast unreliable), seeds A's DB
//! with a document + entity + PII record, fires `StartSync` from B,
//! and asserts that B's DB ends up with all three rows after the
//! `SyncCompleted` event arrives.
//!
//! This exercises the full Phase 3a → 3b → 3c.1 stack:
//!   - SyncManifest covers all five tables (Phase 3a).
//!   - SyncService::get_rows / apply_rows + get_commits / apply_commits
//!     handle the row + commit envelopes (Phase 3b).
//!   - SovereignNode dispatches GetCommits / GetRows requests on the
//!     responder side and chains GetManifest → diff → row/commit
//!     follow-ups → SyncCompleted on the initiator side (Phase 3c.1).

use std::sync::Arc;
use std::time::Duration;

use sovereign_db::mock::MockGraphDB;
use sovereign_db::schema::{Document, Entity, EntityKind, PiiKind, PiiRecord, ReviewState, Thread};
use sovereign_db::GraphDB;
use sovereign_p2p::pairing::PairingManager;
use sovereign_p2p::{P2pCommand, P2pConfig, P2pEvent, SovereignNode, SyncService};
use tokio::sync::mpsc;

/// Build a libp2p Keypair from a fixed seed so peer IDs are stable
/// across test runs.
fn keypair_from_seed(seed: &[u8; 32]) -> libp2p::identity::Keypair {
    libp2p::identity::Keypair::ed25519_from_bytes(*seed)
        .expect("seed is 32 bytes")
}

struct Harness {
    cmd_tx: mpsc::Sender<P2pCommand>,
    event_rx: mpsc::Receiver<P2pEvent>,
    db: Arc<MockGraphDB>,
    peer_id: libp2p::PeerId,
    listen_addr: libp2p::Multiaddr,
}

async fn spawn_node(device_id: &str, seed: [u8; 32], transport_key: [u8; 32]) -> Harness {
    let db = Arc::new(MockGraphDB::new());
    let svc = Arc::new(SyncService::new(
        db.clone() as Arc<dyn GraphDB>,
        device_id.into(),
        transport_key,
    ));

    let (event_tx, event_rx) = mpsc::channel::<P2pEvent>(64);
    let (cmd_tx, cmd_rx) = mpsc::channel::<P2pCommand>(64);

    // Random ephemeral port; loopback only.
    let cfg = P2pConfig {
        enabled: true,
        listen_port: 0,
        rendezvous_server: None,
        device_name: device_id.into(),
        // Allow auto-trigger regardless of host platform — the test
        // doesn't model connectivity transitions.
        wifi_only: false,
    };
    let kp = keypair_from_seed(&seed);
    let peer_id = kp.public().to_peer_id();
    let mut node = SovereignNode::new(&cfg, kp, event_tx, cmd_rx, svc).expect("node");

    let listen_addr = node.listen(&cfg).expect("listen");

    tokio::spawn(async move {
        node.run().await;
    });

    // Give the swarm a moment to actually start listening on the port.
    tokio::time::sleep(Duration::from_millis(200)).await;

    Harness {
        cmd_tx,
        event_rx,
        db,
        peer_id,
        listen_addr,
    }
}

/// Drain events until we see one matching `predicate`, or timeout.
async fn wait_for_event(
    rx: &mut mpsc::Receiver<P2pEvent>,
    timeout: Duration,
    label: &str,
    mut predicate: impl FnMut(&P2pEvent) -> bool,
) -> P2pEvent {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            panic!("timed out waiting for {label}");
        }
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(event)) => {
                if predicate(&event) {
                    return event;
                }
            }
            Ok(None) => panic!("event channel closed waiting for {label}"),
            Err(_) => panic!("timed out waiting for {label}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_nodes_sync_doc_entity_and_pii_record() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info,libp2p_swarm=warn")
        .with_test_writer()
        .try_init();

    // Same transport key on both: paired devices share the AccountKey and
    // therefore derive the same sync transport key (P2P-002).
    let a = spawn_node("device-A", [0xA1; 32], [0x5A; 32]).await;
    let mut b = spawn_node("device-B", [0xB2; 32], [0x5A; 32]).await;

    // ---- Pair the two nodes both ways (P2P-001) ----
    // The responder now refuses sync requests from unpaired peers and the
    // initiator won't dispatch StartSync against one, so each side must
    // know the other before any data can flow.
    a.cmd_tx
        .send(P2pCommand::UpdatePairedPeers {
            peer_ids: vec![b.peer_id.to_string()],
        })
        .await
        .unwrap();
    b.cmd_tx
        .send(P2pCommand::UpdatePairedPeers {
            peer_ids: vec![a.peer_id.to_string()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // ---- Seed A with content B doesn't have ----
    // Thread (so the document has somewhere to live).
    let thread = a
        .db
        .create_thread(Thread::new("Research".into(), String::new()))
        .await
        .unwrap();
    let tid = thread.id_string().unwrap();

    // Document with a commit (sync transports the commit chain).
    let doc = a
        .db
        .create_document(Document::new("Doc-A".into(), tid.clone(), true))
        .await
        .unwrap();
    let doc_id = doc.id_string().unwrap();
    a.db
        .update_document(&doc_id, Some("Doc-A"), Some("body content"))
        .await
        .unwrap();
    a.db.commit_document(&doc_id, "initial").await.unwrap();

    // Entity (LWW row).
    let entity = a
        .db
        .create_entity(Entity::new("Acme Corp".into(), EntityKind::Org))
        .await
        .unwrap();
    let entity_id = entity.id_string().unwrap();

    // PII record (LWW row).
    let pii = a
        .db
        .create_pii_record(PiiRecord {
            id: None,
            kind: PiiKind::Email,
            value_encrypted: "ZmFrZS1jaXBoZXJ0ZXh0".into(),
            value_nonce: String::new(),
            label: Some("work email".into()),
            entity_id: Some(entity_id.clone()),
            stored_secret: false,
            confidence: 0.9,
            sources: vec![],
            discovered_at: chrono::Utc::now(),
            last_revealed_at: None,
            use_count: 0,
            review_state: ReviewState::Confirmed,
            deleted_at: None,
        })
        .await
        .unwrap();
    let pii_id = pii.id_string().unwrap();

    // Sanity: B starts empty.
    assert!(b.db.list_documents(None).await.unwrap().is_empty());
    assert!(b.db.list_entities().await.unwrap().is_empty());
    assert!(b.db.list_pii_records(None, None, None).await.unwrap().is_empty());

    // ---- Connect B → A (no mDNS) ----
    let dial_addr = format!("{}/p2p/{}", a.listen_addr, a.peer_id);
    b.cmd_tx
        .send(P2pCommand::Dial { address: dial_addr })
        .await
        .unwrap();

    // Connection establishment is async; wait briefly for libp2p to
    // negotiate QUIC + Noise. 1s is generous on loopback.
    tokio::time::sleep(Duration::from_millis(800)).await;

    // ---- Trigger sync from B ----
    b.cmd_tx
        .send(P2pCommand::StartSync {
            peer_id: a.peer_id.to_string(),
        })
        .await
        .unwrap();

    // Drain B's events until we see SyncCompleted (or time out).
    let completed = wait_for_event(
        &mut b.event_rx,
        Duration::from_secs(15),
        "B SyncCompleted",
        |e| matches!(e, P2pEvent::SyncCompleted { .. }),
    )
    .await;
    let synced_count = match completed {
        P2pEvent::SyncCompleted { docs_synced, .. } => docs_synced,
        _ => unreachable!(),
    };
    // 1 doc commit + 1 entity row + 1 PII row + 1 thread row = 4 items.
    // The exact count can vary if commit replay produces additional
    // local commits; we just assert "something was synced".
    assert!(
        synced_count >= 3,
        "expected >= 3 items synced, got {synced_count}"
    );

    // ---- Verify B now has the data ----
    let docs = b.db.list_documents(None).await.unwrap();
    assert_eq!(docs.len(), 1, "B should have 1 document after sync");
    // The document content was transmitted via the commit chain; verify
    // the snapshot replayed.
    assert_eq!(docs[0].title, "Doc-A", "doc title should match");
    assert_eq!(docs[0].content, "body content", "doc body should match");

    let entities = b.db.list_entities().await.unwrap();
    assert_eq!(entities.len(), 1, "B should have 1 entity after sync");
    assert_eq!(entities[0].name, "Acme Corp");

    let pii_records = b.db.list_pii_records(None, None, None).await.unwrap();
    assert_eq!(pii_records.len(), 1, "B should have 1 PII record after sync");
    assert_eq!(pii_records[0].kind, PiiKind::Email);
    // The PII record's encrypted value travels as-is (no decrypt during
    // sync); the receiver stores the same ciphertext.
    assert_eq!(pii_records[0].value_encrypted, "ZmFrZS1jaXBoZXJ0ZXh0");

    // Keep the import alive (used by the rejection test below).
    let _ = PairingManager::derive_pair_key(b"unused");

    // We hold doc_id / pii_id in scope for clarity — silence the unused
    // warnings.
    let _ = (doc_id, pii_id);
}

/// P2P-001: a peer that the responder has NOT paired must be refused —
/// it cannot read the responder's DB even though it connects and asks.
/// Here B believes it is paired with A (so it dispatches the request),
/// but A's allow-list is empty, so A returns an Error and B syncs nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unpaired_peer_is_refused() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info,libp2p_swarm=warn")
        .with_test_writer()
        .try_init();

    let a = spawn_node("device-A", [0xC3; 32], [0x6B; 32]).await;
    let mut b = spawn_node("device-B", [0xD4; 32], [0x6B; 32]).await;

    // B is told A is paired (so B will send the request); A is told
    // NOTHING (its allow-list stays empty -> it must refuse B).
    b.cmd_tx
        .send(P2pCommand::UpdatePairedPeers {
            peer_ids: vec![a.peer_id.to_string()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Seed A with a document B must NOT be able to pull.
    let thread = a
        .db
        .create_thread(Thread::new("Secret".into(), String::new()))
        .await
        .unwrap();
    let tid = thread.id_string().unwrap();
    let doc = a
        .db
        .create_document(Document::new("Top-Secret".into(), tid, true))
        .await
        .unwrap();
    let doc_id = doc.id_string().unwrap();
    a.db
        .update_document(&doc_id, Some("Top-Secret"), Some("classified"))
        .await
        .unwrap();
    a.db.commit_document(&doc_id, "initial").await.unwrap();

    // Connect B -> A and attempt to sync.
    let dial_addr = format!("{}/p2p/{}", a.listen_addr, a.peer_id);
    b.cmd_tx
        .send(P2pCommand::Dial { address: dial_addr })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    b.cmd_tx
        .send(P2pCommand::StartSync {
            peer_id: a.peer_id.to_string(),
        })
        .await
        .unwrap();

    // The session still finalizes (A's Error decrements B's pending), so
    // SyncCompleted arrives — but with zero items.
    let completed = wait_for_event(
        &mut b.event_rx,
        Duration::from_secs(15),
        "B SyncCompleted (refused)",
        |e| matches!(e, P2pEvent::SyncCompleted { .. }),
    )
    .await;
    if let P2pEvent::SyncCompleted { docs_synced, .. } = completed {
        assert_eq!(docs_synced, 0, "unpaired peer must receive nothing");
    }

    // Hard assertion: B's DB never received A's secret document.
    let docs = b.db.list_documents(None).await.unwrap();
    assert!(
        docs.is_empty(),
        "B must NOT have pulled any document from an unpairing responder"
    );
}
