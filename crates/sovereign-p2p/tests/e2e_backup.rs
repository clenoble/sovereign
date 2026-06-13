//! End-to-end test of the P4 backup lifecycle.
//!
//! Owner device O prepares a backup (snapshot → seal → erasure-code →
//! Shamir-split the key), places the fragments on two paired host nodes
//! and delivers one guardian key shard to each. Then a "total loss"
//! recovery runs from a blank slate (an ephemeral backup client, no
//! identity): list backups by owner tag, fetch fragments, poll the
//! guardians for the key shards (gated on their approval), assemble +
//! unseal offline, and restore into a fresh DB.
//!
//! Also asserts the storage gates: an UNPAIRED peer cannot place
//! fragments (P2P-001 applies to StoreBackupFragment), and shard
//! release returns nothing before the guardian approves.

use std::sync::Arc;
use std::time::Duration;

use sovereign_db::mock::MockGraphDB;
use sovereign_db::schema::{Contact, Document, Thread};
use sovereign_db::GraphDB;
use sovereign_p2p::backup::{self, BackupGuardianPayload};
use sovereign_p2p::backup_host::BackupHost;
use sovereign_p2p::protocol::SovereignRequest;
use sovereign_p2p::{P2pCommand, P2pConfig, P2pEvent, SovereignNode, SyncService};
use tokio::sync::mpsc;

fn keypair_from_seed(seed: &[u8; 32]) -> libp2p::identity::Keypair {
    libp2p::identity::Keypair::ed25519_from_bytes(*seed).expect("seed is 32 bytes")
}

struct Harness {
    cmd_tx: mpsc::Sender<P2pCommand>,
    event_rx: mpsc::Receiver<P2pEvent>,
    #[allow(dead_code)]
    db: Arc<MockGraphDB>,
    host: Option<Arc<BackupHost>>,
    peer_id: libp2p::PeerId,
    listen_addr: libp2p::Multiaddr,
    host_dir: Option<std::path::PathBuf>,
}

async fn spawn_node(device_id: &str, seed: [u8; 32], with_host: bool) -> Harness {
    let db = Arc::new(MockGraphDB::new());
    let kp = keypair_from_seed(&seed);
    let svc = Arc::new(SyncService::new(
        db.clone() as Arc<dyn GraphDB>,
        // P2P-001: version identity = the verifiable PeerId, not the device id.
        kp.public().to_peer_id().to_string(),
        [0x5A; 32],
        kp.clone(),
        sovereign_p2p::VersionStore::ephemeral(),
    ));

    let (host, host_dir) = if with_host {
        let dir = std::env::temp_dir().join(format!(
            "sovereign-e2e-backup-{device_id}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        // Release delay 0 so the approval gate (not the 72h timer) is
        // what the test exercises; the timer has its own unit test.
        (
            Some(Arc::new(BackupHost::open_with_delay(dir.clone(), 1024 * 1024, 0))),
            Some(dir),
        )
    } else {
        (None, None)
    };

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
    let mut node = SovereignNode::new(&cfg, kp, event_tx, cmd_rx, svc, host.clone())
        .expect("node");
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
        db,
        host,
        peer_id,
        listen_addr,
        host_dir,
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

fn addr_of(h: &Harness) -> String {
    format!("{}/p2p/{}", h.listen_addr, h.peer_id)
}

fn store_requests_for(
    prepared: &backup::PreparedBackup,
    salt_b64: &str,
    fragments: &[backup::BackupFragment],
) -> Vec<SovereignRequest> {
    let manifest_json = prepared.manifest.to_json().unwrap();
    fragments
        .iter()
        .map(|f| SovereignRequest::StoreBackupFragment {
            owner_tag: prepared.manifest.owner_tag.clone(),
            snapshot_id: prepared.manifest.snapshot_id.clone(),
            epoch: prepared.manifest.epoch,
            manifest_json: manifest_json.clone(),
            salt_b64: salt_b64.to_string(),
            index: f.index,
            fragment_b64: f.data_b64.clone(),
            digest: f.digest.clone(),
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn backup_place_and_total_loss_recovery() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info,libp2p_swarm=warn")
        .with_test_writer()
        .try_init();
    use base64::Engine;

    // ---- Owner with content worth backing up ----
    let mut owner = spawn_node("owner", [0xA1; 32], false).await;
    let thread = owner
        .db
        .create_thread(Thread::new("Research".into(), String::new()))
        .await
        .unwrap();
    let tid = thread.id_string().unwrap();
    let mut doc = Document::new("Precious".into(), tid, true);
    doc.content = r#"{"body":"do not lose this","images":[]}"#.into();
    let doc = owner.db.create_document(doc).await.unwrap();
    owner.db.create_contact(Contact::new("Alice".into(), true)).await.unwrap();

    // Two host/guardian nodes.
    let mut h1 = spawn_node("host-1", [0xB2; 32], true).await;
    let h2 = spawn_node("host-2", [0xC3; 32], true).await;

    // Pair owner ↔ hosts (storage is a paired-only commitment).
    for h in [&h1, &h2] {
        owner
            .cmd_tx
            .send(P2pCommand::UpdatePairedPeers {
                peer_ids: vec![h1.peer_id.to_string(), h2.peer_id.to_string()],
            })
            .await
            .unwrap();
        h.cmd_tx
            .send(P2pCommand::UpdatePairedPeers {
                peer_ids: vec![owner.peer_id.to_string()],
            })
            .await
            .unwrap();
    }
    tokio::time::sleep(Duration::from_millis(100)).await;

    // ---- P4.1: prepare the backup ----
    let account = sovereign_crypto::account_key::AccountKey::from_bytes([0x77; 32]);
    let owner_tag = account.derive_backup_tag();
    let salt = b"master-salt".to_vec();
    let guardians = vec!["guardian-h1".to_string(), "guardian-h2".to_string()];
    let prepared = backup::prepare_backup(
        owner.db.as_ref(),
        "owner",
        &owner_tag,
        &salt,
        1,
        &guardians,
        2, // 2-of-2 key threshold
        3,
        2,
    )
    .await
    .unwrap();
    let salt_b64 = base64::engine::general_purpose::STANDARD.encode(&salt);

    // ---- P4.2: place fragments — 0..3 on H1, 2..5 on H2 (overlap on 2) ----
    let dial = |h: &Harness| P2pCommand::Dial { address: addr_of(h) };
    owner.cmd_tx.send(dial(&h1)).await.unwrap();
    owner.cmd_tx.send(dial(&h2)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;

    owner
        .cmd_tx
        .send(P2pCommand::PlaceBackup {
            peer_id: h1.peer_id.to_string(),
            requests: store_requests_for(&prepared, &salt_b64, &prepared.fragments[0..3]),
        })
        .await
        .unwrap();
    let placed = wait_for_event(
        &mut owner.event_rx,
        Duration::from_secs(15),
        "BackupPlaced h1",
        |e| matches!(e, P2pEvent::BackupPlaced { .. }),
    )
    .await;
    if let P2pEvent::BackupPlaced { accepted, rejected, .. } = placed {
        assert_eq!((accepted, rejected), (3, 0), "H1 must accept 3 fragments");
    }

    owner
        .cmd_tx
        .send(P2pCommand::PlaceBackup {
            peer_id: h2.peer_id.to_string(),
            requests: store_requests_for(&prepared, &salt_b64, &prepared.fragments[2..5]),
        })
        .await
        .unwrap();
    let placed = wait_for_event(
        &mut owner.event_rx,
        Duration::from_secs(15),
        "BackupPlaced h2",
        |e| matches!(e, P2pEvent::BackupPlaced { .. }),
    )
    .await;
    if let P2pEvent::BackupPlaced { accepted, rejected, .. } = placed {
        assert_eq!((accepted, rejected), (3, 0), "H2 must accept 3 fragments");
    }

    // Reciprocity accounting is visible on the host.
    let acct = h1.host.as_ref().unwrap().accounting();
    assert_eq!(acct.len(), 1);
    assert_eq!(acct[0].owner_tag, owner_tag);
    assert_eq!(acct[0].fragment_count, 3);

    // ---- P4.1: deliver one guardian key shard to each host ----
    for (i, h) in [&h1, &h2].into_iter().enumerate() {
        let (gid, payload_b64) = &prepared.guardian_payloads[i];
        owner
            .cmd_tx
            .send(P2pCommand::DistributeShard {
                peer_id: h.peer_id.to_string(),
                shard_data: payload_b64.clone(),
                shard_id: format!("{gid}-shard"),
                for_user: owner_tag.clone(),
                epoch: 1,
            })
            .await
            .unwrap();
    }
    let received = wait_for_event(
        &mut h1.event_rx,
        Duration::from_secs(10),
        "ShardReceived on h1",
        |e| matches!(e, P2pEvent::ShardReceived { .. }),
    )
    .await;
    assert!(matches!(received, P2pEvent::ShardReceived { .. }));
    tokio::time::sleep(Duration::from_millis(300)).await;

    // ---- TOTAL LOSS: recover from a blank slate ----
    // The recovering device knows: the passphrase (here: the account key
    // stands in for "derived from passphrase+salt"), and the host
    // addresses. Step 1: any host lists the backup + salt by owner tag.
    let listed = sovereign_p2p::backup_client::list_backups(
        &addr_of(&h1),
        Some(owner_tag.clone()),
        Duration::from_secs(10),
    )
    .await
    .unwrap();
    assert_eq!(listed.len(), 1);
    let info = &listed[0];
    assert_eq!(info.salt_b64, salt_b64, "salt travels with the backup");
    let manifest = backup::BackupManifest::from_json(&info.manifest_json).unwrap();
    assert_eq!(manifest.owner_tag, owner_tag);

    // Step 2: fetch fragments from both hosts (H1 has 0..3, H2 has 2..5).
    let mut fragments = sovereign_p2p::backup_client::fetch_fragments(
        &addr_of(&h1),
        &owner_tag,
        &manifest.snapshot_id,
        &info.fragment_indices,
        &manifest,
        Duration::from_secs(10),
    )
    .await
    .unwrap();
    assert_eq!(fragments.len(), 3);
    let more = sovereign_p2p::backup_client::fetch_fragments(
        &addr_of(&h2),
        &owner_tag,
        &manifest.snapshot_id,
        &[3, 4],
        &manifest,
        Duration::from_secs(10),
    )
    .await
    .unwrap();
    fragments.extend(more);
    assert_eq!(fragments.len(), 5);

    // Step 3: poll guardians. Before approval → nothing, and the host
    // surfaces a ShardRequested event for its user.
    let pending = sovereign_p2p::backup_client::request_guardian_shard(
        &addr_of(&h1),
        "recovery-1",
        &owner_tag,
        1,
        Duration::from_secs(10),
    )
    .await
    .unwrap();
    assert!(pending.is_none(), "shard must be withheld before approval");
    let requested = wait_for_event(
        &mut h1.event_rx,
        Duration::from_secs(10),
        "ShardRequested on h1",
        |e| matches!(e, P2pEvent::ShardRequested { .. }),
    )
    .await;
    if let P2pEvent::ShardRequested { for_user, epoch, .. } = requested {
        assert_eq!(for_user, owner_tag);
        assert_eq!(epoch, 1);
    }

    // Each guardian's user approves (test hosts run with delay 0).
    h1.host.as_ref().unwrap().approve_shard_release(&owner_tag, 1).unwrap();
    h2.host.as_ref().unwrap().approve_shard_release(&owner_tag, 1).unwrap();
    // H2 needs its pending request recorded first, then approval applies.
    let _ = sovereign_p2p::backup_client::request_guardian_shard(
        &addr_of(&h2),
        "recovery-1",
        &owner_tag,
        1,
        Duration::from_secs(10),
    )
    .await
    .unwrap();

    let mut payloads: Vec<BackupGuardianPayload> = Vec::new();
    for h in [&h1, &h2] {
        let p = sovereign_p2p::backup_client::request_guardian_shard(
            &addr_of(h),
            "recovery-1",
            &owner_tag,
            1,
            Duration::from_secs(10),
        )
        .await
        .unwrap()
        .expect("approved shard must be released");
        assert_eq!(p.owner_tag, owner_tag);
        assert_eq!(
            base64::engine::general_purpose::STANDARD.decode(&p.salt_b64).unwrap(),
            salt
        );
        payloads.push(p);
    }

    // Step 4 (offline): assemble + unseal + restore into a fresh DB.
    let snapshot =
        sovereign_p2p::backup_client::assemble_snapshot(&manifest, &fragments, &payloads)
            .unwrap();
    let fresh = MockGraphDB::new();
    let written = backup::restore_snapshot(&fresh, &snapshot).await.unwrap();
    assert!(written >= 3);

    let docs = fresh.list_documents(None).await.unwrap();
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].title, "Precious");
    assert_eq!(docs[0].content, r#"{"body":"do not lose this","images":[]}"#);
    assert_eq!(docs[0].id_string(), doc.id_string(), "origin ids preserved");
    assert_eq!(fresh.list_contacts().await.unwrap().len(), 1);
    assert_eq!(fresh.list_threads().await.unwrap().len(), 1);

    // Cleanup host dirs.
    for h in [&h1, &h2] {
        if let Some(ref dir) = h.host_dir {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unpaired_peer_cannot_store_fragments() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info,libp2p_swarm=warn")
        .with_test_writer()
        .try_init();

    let host = spawn_node("host", [0xD4; 32], true).await;
    let mut stranger = spawn_node("stranger", [0xE5; 32], false).await;
    // Stranger believes the host is paired so it sends; the host has an
    // EMPTY allow-list and must refuse the storage commitment (P2P-001
    // applies to StoreBackupFragment — kills the stranger disk-fill
    // vector too).
    stranger
        .cmd_tx
        .send(P2pCommand::UpdatePairedPeers {
            peer_ids: vec![host.peer_id.to_string()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Seed something to back up + prepare.
    stranger
        .db
        .create_thread(Thread::new("T".into(), String::new()))
        .await
        .unwrap();
    let prepared = backup::prepare_backup(
        stranger.db.as_ref(),
        "stranger",
        "strangertag",
        b"salt",
        1,
        &["g1".to_string(), "g2".to_string()],
        2,
        3,
        2,
    )
    .await
    .unwrap();

    stranger
        .cmd_tx
        .send(P2pCommand::Dial { address: addr_of(&host) })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    stranger
        .cmd_tx
        .send(P2pCommand::PlaceBackup {
            peer_id: host.peer_id.to_string(),
            requests: store_requests_for(&prepared, "c2FsdA", &prepared.fragments),
        })
        .await
        .unwrap();

    let placed = wait_for_event(
        &mut stranger.event_rx,
        Duration::from_secs(15),
        "BackupPlaced (refused)",
        |e| matches!(e, P2pEvent::BackupPlaced { .. }),
    )
    .await;
    if let P2pEvent::BackupPlaced { accepted, rejected, .. } = placed {
        assert_eq!(accepted, 0, "unpaired peer must not store anything");
        assert_eq!(rejected, 5);
    }
    assert!(host.host.as_ref().unwrap().accounting().is_empty());

    if let Some(ref dir) = host.host_dir {
        let _ = std::fs::remove_dir_all(dir);
    }
}
