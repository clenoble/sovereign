//! Manual e2e against the PRODUCTION seed relay (dials the real VM —
//! ignored by default; run with `--ignored` on a connected dev machine).
//!
//! Repro for coord 0040: the app node connects + identifies to the relay
//! but never obtains a reservation, while the M0 spike client (dial →
//! wait for identify → listen_on circuit) gets one in ~50 ms. A granted
//! reservation surfaces as a `/p2p-circuit` ListenAddr event.

use std::sync::Arc;
use std::time::Duration;

use sovereign_db::mock::MockGraphDB;
use sovereign_db::GraphDB;
use sovereign_p2p::{P2pConfig, P2pEvent, SovereignNode, SyncService};
use tokio::sync::mpsc;

const PROD_RELAY: &str =
    "/ip4/92.243.18.31/udp/4001/quic-v1/p2p/12D3KooWGJTBMGBGAArp9S3xZyMsxF5H2SgaDbNLdTz4gXQJpYU9";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "dials the production relay VM — run manually"]
async fn seed_relay_reservation_is_granted() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info,libp2p_swarm=info")
        .with_test_writer()
        .try_init();

    let db = Arc::new(MockGraphDB::new());
    let kp = libp2p::identity::Keypair::generate_ed25519();
    let svc = Arc::new(SyncService::new(
        db as Arc<dyn GraphDB>,
        kp.public().to_peer_id().to_string(),
        [0x7B; 32],
        kp.clone(),
        sovereign_p2p::VersionStore::ephemeral(),
    ));
    let (event_tx, mut event_rx) = mpsc::channel::<P2pEvent>(64);
    let (_cmd_tx, cmd_rx) = mpsc::channel(8);
    let cfg = P2pConfig {
        enabled: true,
        listen_port: 0,
        rendezvous_server: None,
        device_name: "reservation-test".into(),
        enable_mdns: false,
        wifi_only: false,
        seed_relays: vec![PROD_RELAY.to_string()],
    };
    let mut node = SovereignNode::new(&cfg, kp, event_tx, cmd_rx, svc, None).expect("node");
    node.listen(&cfg).expect("listen");
    tokio::spawn(async move { node.run().await });

    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(!remaining.is_zero(), "no /p2p-circuit ListenAddr within 20s — reservation never granted");
        match tokio::time::timeout(remaining, event_rx.recv()).await {
            Ok(Some(P2pEvent::ListenAddr { address })) if address.contains("p2p-circuit") => {
                println!("reservation granted: {address}");
                return;
            }
            Ok(Some(_)) => {}
            Ok(None) => panic!("event channel closed"),
            Err(_) => panic!("no /p2p-circuit ListenAddr within 20s — reservation never granted"),
        }
    }
}
