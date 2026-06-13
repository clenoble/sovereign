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
use sovereign_db::schema::{
    ChannelType, Contact, Conversation, Document, Entity, EntityKind, Message,
    MessageDirection, Milestone, PiiKind, PiiRecord, RelationType, ReviewState,
    SuggestionSource, Thread,
};
use sovereign_db::GraphDB;
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
    svc: Arc<SyncService>,
    peer_id: libp2p::PeerId,
    listen_addr: libp2p::Multiaddr,
}

/// Install the symmetric per-pair sealing key on both ends (P1.4 /
/// P2P-005) the way the app does from the PairingManager: both devices
/// derive the same key from the shared AccountKey + the sorted peer-id
/// pair, no handshake needed.
fn install_pair_keys(a: &Harness, b: &Harness, account_seed: [u8; 32]) {
    let account = sovereign_crypto::account_key::AccountKey::from_bytes(account_seed);
    let pair_key = account.derive_pair_key(&a.peer_id.to_string(), &b.peer_id.to_string());

    let mut a_keys = std::collections::HashMap::new();
    a_keys.insert(b.peer_id.to_string(), pair_key);
    a.svc.set_pair_keys(a_keys);

    let mut b_keys = std::collections::HashMap::new();
    b_keys.insert(a.peer_id.to_string(), pair_key);
    b.svc.set_pair_keys(b_keys);
}

async fn spawn_node(device_id: &str, seed: [u8; 32], transport_key: [u8; 32]) -> Harness {
    let db = Arc::new(MockGraphDB::new());
    // The SyncService signs row envelopes with the SAME keypair the node
    // derives its PeerId from (P1.3) — receivers verify against the
    // sender's peer id, so the two must match.
    let kp = keypair_from_seed(&seed);
    let svc = Arc::new(SyncService::new(
        db.clone() as Arc<dyn GraphDB>,
        // P2P-001: version identity = the verifiable PeerId, not the device id.
        kp.public().to_peer_id().to_string(),
        transport_key,
        kp.clone(),
        sovereign_p2p::VersionStore::ephemeral(),
    ));

    let (event_tx, event_rx) = mpsc::channel::<P2pEvent>(64);
    let (cmd_tx, cmd_rx) = mpsc::channel::<P2pCommand>(64);

    // Random ephemeral port; loopback only.
    let cfg = P2pConfig {
        enabled: true,
        listen_port: 0,
        rendezvous_server: None,
        device_name: device_id.into(),
        // The e2e test relies on mDNS to discover the peer on loopback.
        enable_mdns: true,
        // Allow auto-trigger regardless of host platform — the test
        // doesn't model connectivity transitions.
        wifi_only: false,
    };
    let peer_id = kp.public().to_peer_id();
    let mut node =
        SovereignNode::new(&cfg, kp, event_tx, cmd_rx, svc.clone(), None).expect("node");

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
        svc,
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

    // Per-pair sealing key for rows/commits (P1.4 / P2P-005).
    install_pair_keys(&a, &b, [0x5A; 32]);

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

    // P2 tables: contact, conversation, message, milestone, relationship,
    // suggested link.
    let contact = a.db.create_contact(Contact::new("Alice".into(), true)).await.unwrap();
    let contact_id = contact.id_string().unwrap();
    let conv = a
        .db
        .create_conversation(Conversation::new(
            "Inbox chat".into(),
            ChannelType::Email,
            vec![contact_id.clone()],
        ))
        .await
        .unwrap();
    let conv_id = conv.id_string().unwrap();
    a.db.create_message(Message::new(
        conv_id.clone(),
        ChannelType::Email,
        MessageDirection::Inbound,
        contact_id.clone(),
        vec![],
        "hello from A".into(),
    ))
    .await
    .unwrap();
    a.db.create_milestone(Milestone::new("Kickoff".into(), tid.clone(), String::new()))
        .await
        .unwrap();
    let doc2 = a
        .db
        .create_document(Document::new("Doc-A2".into(), tid.clone(), true))
        .await
        .unwrap();
    let doc2_id = doc2.id_string().unwrap();
    // Documents fetch by head_commit, so an uncommitted doc never syncs
    // (pre-existing limitation of the commit-keyed GetCommits path).
    a.db.commit_document(&doc2_id, "initial").await.unwrap();
    a.db.create_relationship(&doc_id, &doc2_id, RelationType::References, 0.8)
        .await
        .unwrap();
    a.db.create_suggested_link(
        &doc_id,
        &doc2_id,
        RelationType::Supports,
        0.6,
        "both about A",
        SuggestionSource::Consolidation,
    )
    .await
    .unwrap();

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
    assert_eq!(docs.len(), 2, "B should have both documents after sync");
    let doc_a = docs.iter().find(|d| d.title == "Doc-A").expect("Doc-A synced");
    assert_eq!(doc_a.content, "body content", "doc body should match");

    let entities = b.db.list_entities().await.unwrap();
    assert_eq!(entities.len(), 1, "B should have 1 entity after sync");
    assert_eq!(entities[0].name, "Acme Corp");

    let pii_records = b.db.list_pii_records(None, None, None).await.unwrap();
    assert_eq!(pii_records.len(), 1, "B should have 1 PII record after sync");
    assert_eq!(pii_records[0].kind, PiiKind::Email);
    // The PII record's encrypted value travels as-is (no decrypt during
    // sync); the receiver stores the same ciphertext.
    assert_eq!(pii_records[0].value_encrypted, "ZmFrZS1jaXBoZXJ0ZXh0");

    // P2 tables arrived, each under its origin id.
    assert_eq!(b.db.get_contact(&contact_id).await.unwrap().name, "Alice");
    assert_eq!(b.db.get_conversation(&conv_id).await.unwrap().title, "Inbox chat");
    let msgs = b.db.list_all_messages().await.unwrap();
    assert_eq!(msgs.len(), 1, "B should have 1 message after sync");
    assert_eq!(msgs[0].body, "hello from A");
    assert_eq!(b.db.list_all_milestones().await.unwrap().len(), 1);
    let rels = b.db.list_all_relationships().await.unwrap();
    assert_eq!(rels.len(), 1, "B should have the relationship edge");
    assert_eq!(b.db.list_all_suggested_links().await.unwrap().len(), 1);

    // ---- Round 2: a second sync must be a clean no-op (P2 regression:
    // pre-id-preserving creates re-fetched and duplicated rows forever) ----
    b.cmd_tx
        .send(P2pCommand::StartSync {
            peer_id: a.peer_id.to_string(),
        })
        .await
        .unwrap();
    let completed2 = wait_for_event(
        &mut b.event_rx,
        Duration::from_secs(15),
        "B SyncCompleted (round 2)",
        |e| matches!(e, P2pEvent::SyncCompleted { .. }),
    )
    .await;
    if let P2pEvent::SyncCompleted { docs_synced, .. } = completed2 {
        assert_eq!(docs_synced, 0, "second sync round must transfer nothing");
    }
    assert_eq!(b.db.list_documents(None).await.unwrap().len(), 2);
    assert_eq!(b.db.list_threads().await.unwrap().len(), 1);
    assert_eq!(b.db.list_entities().await.unwrap().len(), 1);
    assert_eq!(b.db.list_contacts().await.unwrap().len(), 1);
    assert_eq!(b.db.list_all_messages().await.unwrap().len(), 1);
    assert_eq!(b.db.list_all_milestones().await.unwrap().len(), 1);
    assert_eq!(b.db.list_all_relationships().await.unwrap().len(), 1);
    assert_eq!(b.db.list_all_suggested_links().await.unwrap().len(), 1);
    // And nothing echoed back into A either.
    assert_eq!(a.db.list_documents(None).await.unwrap().len(), 2);
    assert_eq!(a.db.list_contacts().await.unwrap().len(), 1);
    assert_eq!(a.db.list_all_messages().await.unwrap().len(), 1);

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
