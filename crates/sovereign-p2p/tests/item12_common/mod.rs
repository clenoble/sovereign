//! Shared helpers for the v0.0.12 item 12 tests (the phone's P2P node starts
//! without a system DNS config; contract `doc/plans/V0012-item12-phone-p2p.md`).
//!
//! Test-only: a log capture (thread-local subscriber, so the tests must run on
//! a `current_thread` runtime — the node's event loop is spawned on the same
//! thread and its lines land in the buffer), a node spawner, an event waiter.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use sovereign_db::GraphDB;
use sovereign_p2p::{P2pCommand, P2pConfig, P2pEvent, SovereignNode, SyncService};
use tokio::sync::mpsc;

#[path = "../doc_harness/mod.rs"]
mod doc_harness;

pub use doc_harness::encrypted_store;

/// A writer that keeps every formatted log line in memory.
#[derive(Clone, Default)]
pub struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
    /// Number of captured lines containing `needle`.
    pub fn count(&self, needle: &str) -> usize {
        self.text().lines().filter(|l| l.contains(needle)).count()
    }
}

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Capture this thread's log lines: everything the node says (debug and
/// above for `sovereign_p2p`, info and above elsewhere). Keep the guard alive
/// for the whole test.
pub fn capture_logs() -> (Captured, tracing::subscriber::DefaultGuard) {
    let cap = Captured::default();
    let sub = tracing_subscriber::fmt()
        .with_env_filter("info,sovereign_p2p=debug")
        .with_ansi(false)
        .with_writer(cap.clone())
        .finish();
    let guard = tracing::subscriber::set_default(sub);
    (cap, guard)
}

pub fn keypair_from_seed(seed: &[u8; 32]) -> libp2p::identity::Keypair {
    libp2p::identity::Keypair::ed25519_from_bytes(*seed).expect("seed is 32 bytes")
}

pub fn test_config(device: &str, enable_mdns: bool) -> P2pConfig {
    P2pConfig {
        enabled: true,
        listen_port: 0,
        rendezvous_server: None,
        device_name: device.into(),
        enable_mdns,
        wifi_only: false,
        seed_relays: Vec::new(),
    }
}

pub struct Harness {
    pub cmd_tx: mpsc::Sender<P2pCommand>,
    pub event_rx: mpsc::Receiver<P2pEvent>,
    pub db: Arc<sovereign_db::encrypted::EncryptedGraphDB>,
    pub svc: Arc<SyncService>,
    pub peer_id: libp2p::PeerId,
    /// A concrete loopback address the node reported (`/ip4/127.0.0.1/udp/<port>/quic-v1`).
    pub loopback_addr: libp2p::Multiaddr,
    /// What the built node said about its DNS transport, read before `run()`.
    pub has_dns: bool,
}

pub struct Parts {
    pub cfg: P2pConfig,
    pub kp: libp2p::identity::Keypair,
    pub event_tx: mpsc::Sender<P2pEvent>,
    pub event_rx: mpsc::Receiver<P2pEvent>,
    pub cmd_tx: mpsc::Sender<P2pCommand>,
    /// Taken by the constructor under test (`p.cmd_rx.take().unwrap()`).
    pub cmd_rx: Option<mpsc::Receiver<P2pCommand>>,
    pub db: Arc<sovereign_db::encrypted::EncryptedGraphDB>,
    pub svc: Arc<SyncService>,
}

/// Everything `SovereignNode::new*` takes, built the way the e2e tests do.
pub async fn parts(device: &str, seed: [u8; 32], transport_key: [u8; 32], enable_mdns: bool) -> Parts {
    let (db, docs) = encrypted_store(device).await;
    let kp = keypair_from_seed(&seed);
    let svc = Arc::new(
        SyncService::new(
            db.clone() as Arc<dyn GraphDB>,
            kp.public().to_peer_id().to_string(),
            transport_key,
            kp.clone(),
            sovereign_p2p::VersionStore::ephemeral(),
        )
        .with_documents(docs),
    );
    let (event_tx, event_rx) = mpsc::channel::<P2pEvent>(256);
    let (cmd_tx, cmd_rx) = mpsc::channel::<P2pCommand>(64);
    Parts {
        cfg: test_config(device, enable_mdns),
        kp,
        event_tx,
        event_rx,
        cmd_tx,
        cmd_rx: Some(cmd_rx),
        db,
        svc,
    }
}

/// Listen, spawn the event loop, wait for the loopback address.
pub async fn start(
    mut node: SovereignNode,
    p: Parts,
    peer_id: libp2p::PeerId,
    has_dns: bool,
) -> Harness {
    node.listen(&p.cfg).expect("listen");
    tokio::spawn(async move { node.run().await });
    let mut event_rx = p.event_rx;
    let ev = wait_for_event(&mut event_rx, Duration::from_secs(10), "loopback ListenAddr", |e| {
        matches!(e, P2pEvent::ListenAddr { address } if address.contains("/ip4/127.0.0.1/"))
    })
    .await;
    let loopback_addr: libp2p::Multiaddr = match ev {
        P2pEvent::ListenAddr { address } => address.parse().expect("multiaddr"),
        _ => unreachable!(),
    };
    Harness {
        cmd_tx: p.cmd_tx,
        event_rx,
        db: p.db,
        svc: p.svc,
        peer_id,
        loopback_addr,
        has_dns,
    }
}

pub async fn wait_for_event(
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

/// The UDP port of a `/ip4/127.0.0.1/udp/<port>/quic-v1` address.
pub fn udp_port(addr: &libp2p::Multiaddr) -> u16 {
    addr.iter()
        .find_map(|p| match p {
            libp2p::multiaddr::Protocol::Udp(port) => Some(port),
            _ => None,
        })
        .expect("udp port in listen address")
}

/// Wait up to `within` for a log line, polling the capture. True if seen.
pub async fn log_seen(cap: &Captured, needle: &str, within: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + within;
    while tokio::time::Instant::now() < deadline {
        if cap.count(needle) > 0 {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    cap.count(needle) > 0
}

/// Wait up to `within` for the count of lines containing `needle` to exceed
/// `baseline`. True if it did.
pub async fn log_grew(cap: &Captured, needle: &str, baseline: usize, within: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + within;
    while tokio::time::Instant::now() < deadline {
        if cap.count(needle) > baseline {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    cap.count(needle) > baseline
}

/// Install the per-pair sealing key on both ends and pair them both ways
/// (P1.4 / P2P-005, P2P-001), as the e2e sync test does.
pub async fn pair(a: &Harness, b: &Harness, account_seed: [u8; 32]) {
    let account = sovereign_crypto::account_key::AccountKey::from_bytes(account_seed);
    let pair_key = account.derive_pair_key(&a.peer_id.to_string(), &b.peer_id.to_string());
    let mut a_keys = std::collections::HashMap::new();
    a_keys.insert(b.peer_id.to_string(), pair_key);
    a.svc.set_pair_keys(a_keys);
    let mut b_keys = std::collections::HashMap::new();
    b_keys.insert(a.peer_id.to_string(), pair_key);
    b.svc.set_pair_keys(b_keys);
    a.cmd_tx
        .send(P2pCommand::UpdatePairedPeers { peer_ids: vec![b.peer_id.to_string()] })
        .await
        .unwrap();
    b.cmd_tx
        .send(P2pCommand::UpdatePairedPeers { peer_ids: vec![a.peer_id.to_string()] })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
}
