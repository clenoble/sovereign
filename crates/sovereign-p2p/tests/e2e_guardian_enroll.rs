//! End-to-end test of the G2 guardian enrollment handshake.
//!
//! Spins up an "owner" node armed with an enrollment offer (the way the
//! app will when rendering the QR), then runs the guardian-side client
//! against it:
//!   - happy path: Hello → Challenge → Proof → Granted → Complete →
//!     Done; asserts the released grant, that the owner emits
//!     `GuardianEnrolled` with the guardian's persistent identity, and —
//!     role separation — that the enrolled guardian is still refused
//!     sync (a guardian is NOT a paired device);
//!   - wrong-code path: rejected proofs burn attempts, the offer
//!     self-destructs, the right code is refused afterwards.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use sovereign_db::mock::MockGraphDB;
use sovereign_db::GraphDB;
use sovereign_p2p::guardian_enroll::{
    self, GuardianEnrollOffer, GUARDIAN_OFFER_TTL_SECONDS,
};
use sovereign_p2p::protocol::SovereignRequest;
use sovereign_p2p::{
    ActiveGuardianOffer, P2pCommand, P2pConfig, P2pEvent, SovereignNode, SyncService,
};
use tokio::sync::mpsc;

fn keypair_from_seed(seed: &[u8; 32]) -> libp2p::identity::Keypair {
    libp2p::identity::Keypair::ed25519_from_bytes(*seed).expect("seed is 32 bytes")
}

struct Harness {
    cmd_tx: mpsc::Sender<P2pCommand>,
    event_rx: mpsc::Receiver<P2pEvent>,
    peer_id: libp2p::PeerId,
    listen_addr: libp2p::Multiaddr,
}

async fn spawn_node(device_id: &str, seed: [u8; 32]) -> Harness {
    let db = Arc::new(MockGraphDB::new());
    let kp = keypair_from_seed(&seed);
    let svc = Arc::new(SyncService::new(
        db.clone() as Arc<dyn GraphDB>,
        kp.public().to_peer_id().to_string(),
        [0x5A; 32],
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
        seed_relays: Vec::new(),
    };
    let peer_id = kp.public().to_peer_id();
    let mut node =
        SovereignNode::new(&cfg, kp, event_tx, cmd_rx, svc.clone(), None).expect("node");
    node.listen(&cfg).expect("listen");
    tokio::spawn(async move {
        node.run().await;
    });

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

const SHARD: [u8; 33] = [0xAB; 33];

/// Arm the owner node with an enrollment offer the way the app will:
/// build the QR offer, stretch the code, hand the node the handshake key
/// + the shard to release. Returns the (QR-encodable) offer.
async fn arm_offer(owner: &Harness, code: &str) -> GuardianEnrollOffer {
    let offer = GuardianEnrollOffer::new(
        owner.peer_id.to_string(),
        "Céline".into(),
        vec![owner.listen_addr.to_string()],
        GUARDIAN_OFFER_TTL_SECONDS,
    );
    let handshake_key = guardian_enroll::derive_enroll_key(code, &offer).unwrap();
    owner
        .cmd_tx
        .send(P2pCommand::SetGuardianOffer {
            offer: Box::new(ActiveGuardianOffer::new(
                offer.offer_id.clone(),
                handshake_key,
                offer.expires_at,
                base64::engine::general_purpose::STANDARD.encode(SHARD),
                "shard-3".into(),
                "owner-tag-1".into(),
                "Céline".into(),
                7,
                3,
                5,
            )),
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    offer
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn guardian_enrollment_end_to_end() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info,libp2p_swarm=warn")
        .with_test_writer()
        .try_init();

    let mut owner = spawn_node("owner-device", [0xA1; 32]).await;
    let code = sovereign_crypto::pair_payload::generate_pairing_code();
    let offer = arm_offer(&owner, &code).await;

    // QR round-trip sanity (what the guardian app scans).
    let offer = GuardianEnrollOffer::decode(&offer.encode().unwrap()).unwrap();
    assert_eq!(offer.owner_label, "Céline");

    // The guardian's PERSISTENT identity — no rebind, unlike pairing.
    let guardian_kp = keypair_from_seed(&[0xB2; 32]);
    let guardian_peer_id = guardian_kp.public().to_peer_id().to_string();

    let mut persisted: Option<guardian_enroll::GuardianGrant> = None;
    let outcome = guardian_enroll::enroll_with_owner(
        &offer,
        &code,
        guardian_kp,
        "Ami·e",
        |grant| {
            persisted = Some(grant.clone());
            Ok(())
        },
        Duration::from_secs(15),
    )
    .await
    .expect("enrollment should succeed");

    // The grant carries the shard + custody metadata.
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(&outcome.grant.shard_b64)
            .unwrap(),
        SHARD
    );
    assert_eq!(outcome.grant.shard_id, "shard-3");
    assert_eq!(outcome.grant.owner_tag, "owner-tag-1");
    assert_eq!(outcome.grant.epoch, 7);
    assert_eq!((outcome.grant.threshold, outcome.grant.total), (3, 5));
    assert!(persisted.is_some(), "persist callback ran before the receipt");

    // Owner emitted GuardianEnrolled with the guardian's identity.
    let enrolled = wait_for_event(
        &mut owner.event_rx,
        Duration::from_secs(5),
        "GuardianEnrolled on owner",
        |e| matches!(e, P2pEvent::GuardianEnrolled { .. }),
    )
    .await;
    if let P2pEvent::GuardianEnrolled {
        guardian_peer_id: pid,
        guardian_label,
        shard_id,
        epoch,
    } = enrolled
    {
        assert_eq!(pid, guardian_peer_id);
        assert_eq!(guardian_label, "Ami·e");
        assert_eq!(shard_id, "shard-3");
        assert_eq!(epoch, 7);
    }

    // Role separation: enrollment must NOT have paired the guardian.
    // A guardian node dialing the owner and asking for sync data is
    // refused exactly like any stranger (P2P-001).
    let mut guardian_node = spawn_node("guardian-device", [0xB2; 32]).await;
    let dial_addr = format!("{}/p2p/{}", owner.listen_addr, owner.peer_id);
    guardian_node
        .cmd_tx
        .send(P2pCommand::Dial { address: dial_addr })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    guardian_node
        .cmd_tx
        .send(P2pCommand::SendRequest {
            peer_id: owner.peer_id,
            request: SovereignRequest::GetManifest,
        })
        .await
        .unwrap();
    // The refusal arrives as an untracked response on the guardian's
    // node; what matters is observable on the owner side: no sync
    // session ever starts. Give it a beat, then assert the owner never
    // emitted SyncStarted.
    tokio::time::sleep(Duration::from_millis(500)).await;
    while let Ok(e) = owner.event_rx.try_recv() {
        assert!(
            !matches!(e, P2pEvent::SyncStarted { .. } | P2pEvent::SyncCompleted { .. }),
            "guardian must not be able to sync (not a paired device)"
        );
    }

    // Single-use: replaying the handshake against the consumed offer dies.
    let replay_kp = keypair_from_seed(&[0xC3; 32]);
    let err = guardian_enroll::enroll_with_owner(
        &offer,
        &code,
        replay_kp,
        "Replayer",
        |_| Ok(()),
        Duration::from_secs(10),
    )
    .await
    .expect_err("offer must be consumed after a successful enrollment");
    assert!(format!("{err}").contains("rejected"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wrong_code_burns_attempts_and_kills_offer() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info,libp2p_swarm=warn")
        .with_test_writer()
        .try_init();

    let mut owner = spawn_node("owner-device", [0xD4; 32]).await;
    let code = sovereign_crypto::pair_payload::generate_pairing_code();
    let offer = arm_offer(&owner, &code).await;

    for attempt in 1..=3u8 {
        // Fresh ephemeral guardian identity per attempt — an attacker
        // isn't obliged to reuse a peer id.
        let kp = libp2p::identity::Keypair::generate_ed25519();
        let err = guardian_enroll::enroll_with_owner(
            &offer,
            "WRONG-CODES",
            kp,
            "Imposter",
            |_| Ok(()),
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
        &mut owner.event_rx,
        Duration::from_secs(5),
        "offer_dead GuardianEnrollFailed",
        |e| matches!(e, P2pEvent::GuardianEnrollFailed { offer_dead: true, .. }),
    )
    .await;
    assert!(matches!(failed, P2pEvent::GuardianEnrollFailed { .. }));

    // The RIGHT code is now useless: the offer is gone.
    let kp = libp2p::identity::Keypair::generate_ed25519();
    let err = guardian_enroll::enroll_with_owner(
        &offer,
        &code,
        kp,
        "Late legit guardian",
        |_| Ok(()),
        Duration::from_secs(10),
    )
    .await
    .expect_err("offer must be dead after attempt exhaustion");
    assert!(format!("{err}").contains("rejected"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn persist_failure_aborts_before_receipt() {
    // If the guardian can't durably store the shard, the handshake must
    // fail on the guardian side AND the owner must never see a custody
    // receipt — no GuardianEnrolled, roster stays empty.
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info,libp2p_swarm=warn")
        .with_test_writer()
        .try_init();

    let mut owner = spawn_node("owner-device", [0xE5; 32]).await;
    let code = sovereign_crypto::pair_payload::generate_pairing_code();
    let offer = arm_offer(&owner, &code).await;

    let kp = libp2p::identity::Keypair::generate_ed25519();
    let err = guardian_enroll::enroll_with_owner(
        &offer,
        &code,
        kp,
        "Full disk",
        |_| Err("disk full".into()),
        Duration::from_secs(10),
    )
    .await
    .expect_err("persist failure must abort enrollment");
    assert!(format!("{err}").contains("persist"));

    tokio::time::sleep(Duration::from_millis(500)).await;
    while let Ok(e) = owner.event_rx.try_recv() {
        assert!(
            !matches!(e, P2pEvent::GuardianEnrolled { .. }),
            "owner must not record custody that was never confirmed"
        );
    }
}
