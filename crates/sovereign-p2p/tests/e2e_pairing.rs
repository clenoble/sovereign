//! End-to-end test of the P3.1 interactive pairing handshake.
//!
//! Spins up an "existing device" node (A), arms it with a pairing offer
//! the way the app does when rendering the QR, then runs the new-device
//! client against it:
//!   - the full happy path (Hello → Challenge → Proof → Granted →
//!     Complete → Done), asserting the released secrets, that A emits
//!     `PairingCompleted` with the new device's FINAL identity, and that
//!     this identity can immediately sync with A (allow-list + pair key
//!     installed by the handshake, no app round-trip needed);
//!   - the wrong-code path: rejected proofs burn attempts, the offer
//!     self-destructs after MAX_PROOF_ATTEMPTS, and the right code is
//!     refused afterwards.

use std::sync::Arc;
use std::time::Duration;

use sovereign_db::mock::MockGraphDB;
use sovereign_db::schema::{Document, Thread};
use sovereign_db::GraphDB;
use sovereign_p2p::pairing_offer::{self, PairingOffer, OFFER_TTL_SECONDS};
use sovereign_p2p::{
    ActivePairingOffer, P2pCommand, P2pConfig, P2pEvent, SovereignNode, SyncService,
};
use tokio::sync::mpsc;

fn keypair_from_seed(seed: &[u8; 32]) -> libp2p::identity::Keypair {
    libp2p::identity::Keypair::ed25519_from_bytes(*seed).expect("seed is 32 bytes")
}

struct Harness {
    cmd_tx: mpsc::Sender<P2pCommand>,
    event_rx: mpsc::Receiver<P2pEvent>,
    db: Arc<MockGraphDB>,
    svc: Arc<SyncService>,
    peer_id: libp2p::PeerId,
    listen_addr: libp2p::Multiaddr,
}

async fn spawn_node(device_id: &str, seed: [u8; 32], transport_key: [u8; 32]) -> Harness {
    let db = Arc::new(MockGraphDB::new());
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

    let cfg = P2pConfig {
        enabled: true,
        listen_port: 0,
        rendezvous_server: None,
        device_name: device_id.into(),
        enable_mdns: false,
        wifi_only: false,
    };
    let peer_id = kp.public().to_peer_id();
    let mut node =
        SovereignNode::new(&cfg, kp, event_tx, cmd_rx, svc.clone(), None).expect("node");
    node.listen(&cfg).expect("listen");
    tokio::spawn(async move {
        node.run().await;
    });

    // The configured bind addr is 0.0.0.0:0 — wait for the node to
    // report a concrete loopback address we can actually dial.
    let mut event_rx = event_rx;
    let listen_addr = wait_for_event(
        &mut event_rx,
        Duration::from_secs(5),
        "loopback ListenAddr",
        |e| matches!(e, P2pEvent::ListenAddr { address } if address.contains("127.0.0.1")),
    )
    .await;
    let listen_addr: libp2p::Multiaddr = match listen_addr {
        P2pEvent::ListenAddr { address } => address.parse().expect("multiaddr"),
        _ => unreachable!(),
    };

    Harness {
        cmd_tx,
        event_rx,
        db,
        svc,
        peer_id,
        listen_addr,
    }
}

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

/// Arm node A with a pairing offer the way `generate_pair_qr` will:
/// build the offer, stretch the code, hand the node the handshake key +
/// the secrets to release. Returns the (QR-encodable) offer.
async fn arm_offer(a: &Harness, code: &str, account_key: [u8; 32], salt: &[u8]) -> PairingOffer {
    let offer = PairingOffer::new(
        a.peer_id.to_string(),
        "Existing device".into(),
        vec![a.listen_addr.to_string()],
        OFFER_TTL_SECONDS,
    );
    let handshake_key = pairing_offer::derive_handshake_key(code, &offer).unwrap();
    a.cmd_tx
        .send(P2pCommand::SetPairingOffer {
            offer: Box::new(ActivePairingOffer::new(
                offer.offer_id.clone(),
                handshake_key,
                offer.expires_at,
                salt.to_vec(),
                account_key,
                "Existing device".into(),
            )),
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    offer
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pairing_handshake_end_to_end_then_sync() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info,libp2p_swarm=warn")
        .with_test_writer()
        .try_init();

    const ACCOUNT_KEY: [u8; 32] = [0x5A; 32];
    let mut a = spawn_node("device-A", [0xA1; 32], [0x5A; 32]).await;

    // Seed A with content the freshly paired device should receive.
    let thread = a
        .db
        .create_thread(Thread::new("Research".into(), String::new()))
        .await
        .unwrap();
    let tid = thread.id_string().unwrap();
    let doc = a
        .db
        .create_document(Document::new("Doc-A".into(), tid, true))
        .await
        .unwrap();
    a.db.commit_document(&doc.id_string().unwrap(), "initial")
        .await
        .unwrap();

    let code = sovereign_crypto::pair_payload::generate_pairing_code();
    let offer = arm_offer(&a, &code, ACCOUNT_KEY, b"master-salt").await;

    // QR sanity: encodes/decodes, carries no secrets (asserted in unit
    // tests), and round-trips through the string the UI would render.
    let offer = PairingOffer::decode(&offer.encode().unwrap()).unwrap();

    // The "final identity" the new device will derive once it has the
    // salt. In the app this comes from DeviceKey::derive(master, dev_id);
    // here a fixed keypair stands in for it.
    let final_kp = keypair_from_seed(&[0xC3; 32]);
    let final_peer_id = final_kp.public().to_peer_id().to_string();

    let outcome = sovereign_p2p::pairing_client::pair_with_source(
        &offer,
        &code,
        "New phone",
        |secrets| {
            assert_eq!(secrets.salt, b"master-salt");
            Ok(final_kp.public().to_peer_id().to_string())
        },
        Duration::from_secs(15),
    )
    .await
    .expect("handshake should succeed");

    assert_eq!(outcome.secrets.account_key_bytes, ACCOUNT_KEY);
    assert_eq!(outcome.secrets.salt, b"master-salt");
    assert_eq!(outcome.secrets.source_device_name, "Existing device");
    assert_eq!(outcome.final_peer_id, final_peer_id);

    // A announced the pairing with the FINAL identity.
    let completed = wait_for_event(
        &mut a.event_rx,
        Duration::from_secs(5),
        "PairingCompleted on A",
        |e| matches!(e, P2pEvent::PairingCompleted { .. }),
    )
    .await;
    if let P2pEvent::PairingCompleted { peer_id, device_name } = completed {
        assert_eq!(peer_id, final_peer_id);
        assert_eq!(device_name, "New phone");
    }

    // The handshake must have armed A for the new device end-to-end:
    // bring up a node under the final identity and sync from A without
    // ever touching A's command channel again.
    let mut b = spawn_node("device-B", [0xC3; 32], [0x5A; 32]).await;
    assert_eq!(b.peer_id.to_string(), final_peer_id);
    // B knows A (the app persists this from the handshake outcome).
    b.cmd_tx
        .send(P2pCommand::UpdatePairedPeers {
            peer_ids: vec![a.peer_id.to_string()],
        })
        .await
        .unwrap();
    let account = sovereign_crypto::account_key::AccountKey::from_bytes(ACCOUNT_KEY);
    let pair_key = account.derive_pair_key(&a.peer_id.to_string(), &final_peer_id);
    let mut keys = std::collections::HashMap::new();
    keys.insert(a.peer_id.to_string(), pair_key);
    b.svc.set_pair_keys(keys);

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

    let completed = wait_for_event(
        &mut b.event_rx,
        Duration::from_secs(15),
        "B SyncCompleted after pairing",
        |e| matches!(e, P2pEvent::SyncCompleted { .. }),
    )
    .await;
    if let P2pEvent::SyncCompleted { docs_synced, .. } = completed {
        assert!(
            docs_synced >= 1,
            "freshly paired device must be able to sync (allow-list + pair key installed by the handshake)"
        );
    }
    assert_eq!(
        b.db.list_documents(None).await.unwrap().len(),
        1,
        "A's document must arrive on the freshly paired device"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_code_burns_attempts_and_kills_offer() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info,libp2p_swarm=warn")
        .with_test_writer()
        .try_init();

    let mut a = spawn_node("device-A", [0xD4; 32], [0x6B; 32]).await;
    let code = sovereign_crypto::pair_payload::generate_pairing_code();
    let offer = arm_offer(&a, &code, [0x6B; 32], b"salt").await;

    // Three wrong-code attempts: each must be rejected, the third kills
    // the offer.
    for attempt in 1..=3u8 {
        let err = sovereign_p2p::pairing_client::pair_with_source(
            &offer,
            "WRONG-CODES",
            "Imposter",
            |_| Ok("12D3KooWNever".into()),
            Duration::from_secs(10),
        )
        .await
        .expect_err("wrong code must fail");
        let msg = format!("{err}");
        assert!(
            msg.contains("rejected"),
            "attempt {attempt}: expected rejection, got: {msg}"
        );
    }
    let failed = wait_for_event(
        &mut a.event_rx,
        Duration::from_secs(5),
        "offer_dead PairingFailed",
        |e| matches!(e, P2pEvent::PairingFailed { offer_dead: true, .. }),
    )
    .await;
    assert!(matches!(failed, P2pEvent::PairingFailed { .. }));

    // The RIGHT code is now useless: the offer is gone.
    let err = sovereign_p2p::pairing_client::pair_with_source(
        &offer,
        &code,
        "Late legit device",
        |_| Ok("12D3KooWNever".into()),
        Duration::from_secs(10),
    )
    .await
    .expect_err("offer must be dead after attempt exhaustion");
    assert!(format!("{err}").contains("rejected"));
}
