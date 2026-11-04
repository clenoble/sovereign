//! v0.0.12 item 12 — the phone's P2P node starts without a system DNS config
//! (contract: `doc/plans/V0012-item12-phone-p2p.md`, P1–P3; P5 is the manual
//! phone run, P4 is the frontend test `SettingsPanel.item12.svelte.test.ts`).
//!
//! The "system resolver config" is injected, never chosen by `target_os` (P2):
//! the phone's case is reproduced on this desktop by a source that fails the
//! way hickory's `read_system_conf` fails on Android (no `/etc/resolv.conf`).
//!
//! API these tests expect (does not exist yet — the item provides it):
//!   * `SovereignNode::new_with_dns_source(config, keypair, event_tx,
//!      command_rx, sync_service, backup_host, source)` where `source` is a
//!      `fn() -> std::io::Result<(ResolverConfig, ResolverOpts)>` (the types
//!      re-exported as `sovereign_p2p::libp2p::dns::{ResolverConfig,
//!      ResolverOpts}`). It is consulted once, while the swarm is built.
//!      `SovereignNode::new` keeps its signature and uses the real system
//!      source.
//!   * `SovereignNode::has_dns_transport(&self) -> bool` — how the node
//!     "reports that it runs without DNS"; the app reads it before `run()`
//!     and carries it to Settings → Devices (P4).
//!
//! Log assertions use a thread-local capture, hence the `current_thread`
//! runtimes: the node's event loop runs on the test's thread.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

mod item12_common;
use item12_common::*;

use sovereign_db::schema::Thread;
use sovereign_db::GraphDB;
use sovereign_p2p::libp2p::dns::{ResolverConfig, ResolverOpts};
use sovereign_p2p::{P2pCommand, P2pEvent, SovereignNode};

/// The plain reason of P3, as the contract words it: "this device cannot
/// resolve names; it syncs on the local network and with addresses given by
/// IP". The tests match its first clause.
const PLAIN_REASON: &str = "cannot resolve names";
// The counts below are taken from a baseline read after the node started, so a
// startup line that uses the same words (P1's report, P5's "log line") neither
// satisfies nor breaks the dial assertions: the dial itself must log the reason.

// ---- the injected "system DNS config" sources -------------------------------

static NO_DNS_CALLS: AtomicUsize = AtomicUsize::new(0);
static WITH_DNS_CALLS: AtomicUsize = AtomicUsize::new(0);

/// Android: hickory's unix reader finds no `/etc/resolv.conf`.
fn no_system_dns() -> std::io::Result<(ResolverConfig, ResolverOpts)> {
    NO_DNS_CALLS.fetch_add(1, Ordering::SeqCst);
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "No such file or directory (os error 2): /etc/resolv.conf",
    ))
}

/// A readable system config. Empty name-server list on purpose: nothing is
/// ever sent to a resolver by these tests.
fn some_system_dns() -> std::io::Result<(ResolverConfig, ResolverOpts)> {
    WITH_DNS_CALLS.fetch_add(1, Ordering::SeqCst);
    Ok((ResolverConfig::new(), ResolverOpts::default()))
}

// ---- spawning -----------------------------------------------------------------

async fn spawn_with_source(
    device: &str,
    seed: [u8; 32],
    transport_key: [u8; 32],
    enable_mdns: bool,
    source: fn() -> std::io::Result<(ResolverConfig, ResolverOpts)>,
) -> Harness {
    let mut p = parts(device, seed, transport_key, enable_mdns).await;
    let peer_id = p.kp.public().to_peer_id();
    let cmd_rx = p.cmd_rx.take().unwrap();
    let node = SovereignNode::new_with_dns_source(
        &p.cfg,
        p.kp.clone(),
        p.event_tx.clone(),
        cmd_rx,
        p.svc.clone(),
        None,
        source,
    )
    .expect("the node builds");
    let has_dns = node.has_dns_transport();
    start(node, p, peer_id, has_dns).await
}

// ---- P1 — the node starts without a system DNS config ------------------------

/// P1 — without a readable system resolver config the node is built (mDNS on,
/// as on the phone), says it has no DNS transport, listens and runs.
#[tokio::test(flavor = "current_thread")]
async fn p1_node_builds_and_starts_without_system_dns_config() {
    let before = NO_DNS_CALLS.load(Ordering::SeqCst);
    let mut p = parts("item12-p1-phone", [0x31; 32], [0x13; 32], true).await;
    let peer_id = p.kp.public().to_peer_id();
    let cmd_rx = p.cmd_rx.take().unwrap();

    let built = SovereignNode::new_with_dns_source(
        &p.cfg,
        p.kp.clone(),
        p.event_tx.clone(),
        cmd_rx,
        p.svc.clone(),
        None,
        no_system_dns,
    );
    let node = match built {
        Ok(n) => n,
        Err(e) => panic!("P1: a missing system DNS config must not stop the node: {e}"),
    };
    assert!(
        NO_DNS_CALLS.load(Ordering::SeqCst) > before,
        "P2: the choice is made by trying the system config (the injected source was never consulted)"
    );
    assert!(!node.has_dns_transport(), "P1: the node reports that it runs without DNS");

    // It starts: listens, and its event loop reports a concrete address.
    let h = start(node, p, peer_id, false).await;
    assert!(h.loopback_addr.to_string().starts_with("/ip4/127.0.0.1/udp/"));
    h.cmd_tx.send(P2pCommand::Shutdown).await.unwrap();
}

/// P1 — two nodes without DNS, paired, reach each other by IP and sync: the
/// LAN path the phone needs is untouched by the missing transport.
#[tokio::test(flavor = "current_thread")]
async fn p1_two_nodes_without_dns_connect_by_ip_and_sync() {
    let a = spawn_with_source("item12-p1-A", [0x41; 32], [0x14; 32], false, no_system_dns).await;
    let mut b = spawn_with_source("item12-p1-B", [0x42; 32], [0x14; 32], false, no_system_dns).await;
    assert!(!a.has_dns && !b.has_dns, "both nodes run without DNS");

    pair(&a, &b, [0x14; 32]).await;
    a.db
        .create_thread(Thread::new("Item12 by IP".into(), String::new()))
        .await
        .unwrap();

    // B dials A by IP (loopback stands for the LAN address), then syncs.
    let by_ip = format!("{}/p2p/{}", a.loopback_addr, a.peer_id);
    b.cmd_tx.send(P2pCommand::Dial { address: by_ip }).await.unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;
    b.cmd_tx
        .send(P2pCommand::StartSync { peer_id: a.peer_id.to_string() })
        .await
        .unwrap();
    wait_for_event(&mut b.event_rx, Duration::from_secs(20), "B SyncCompleted", |e| {
        matches!(e, P2pEvent::SyncCompleted { .. })
    })
    .await;

    let names: Vec<String> = b.db.list_threads().await.unwrap().into_iter().map(|t| t.name).collect();
    assert!(
        names.iter().any(|n| n == "Item12 by IP"),
        "P1: B (no DNS) received A's (no DNS) thread over an IP dial; B has {names:?}"
    );
}

// ---- P2 — only when it is missing -------------------------------------------

/// P2 — a readable system config (injected) gives the DNS transport, as today.
/// Same desktop, same build: only the source differs, so the choice cannot be
/// a `target_os` switch.
#[tokio::test(flavor = "current_thread")]
async fn p2_readable_system_dns_config_builds_the_dns_transport() {
    let before = WITH_DNS_CALLS.load(Ordering::SeqCst);
    let h = spawn_with_source("item12-p2-desk", [0x51; 32], [0x15; 32], false, some_system_dns).await;
    assert!(WITH_DNS_CALLS.load(Ordering::SeqCst) > before, "P2: the source is consulted");
    assert!(h.has_dns, "P2: with a system config the DNS transport is built as today");
}

/// P2 — `SovereignNode::new` (what the app and the shell call) still builds
/// the DNS transport on a host that has a system config. The behavioural half
/// (a named peer is reached) is `item12_p2_dns_control.rs`, which runs today.
#[tokio::test(flavor = "current_thread")]
async fn p2_default_constructor_keeps_dns_where_the_system_has_a_config() {
    let mut p = parts("item12-p2-default", [0x52; 32], [0x15; 32], false).await;
    let cmd_rx = p.cmd_rx.take().unwrap();
    let node = SovereignNode::new(&p.cfg, p.kp.clone(), p.event_tx.clone(), cmd_rx, p.svc.clone(), None)
        .expect("node builds on this desktop");
    assert!(
        node.has_dns_transport(),
        "P2: on a host with a system resolver config (this desktop) SovereignNode::new keeps DNS"
    );
}

// ---- P3 — named addresses say why they fail -----------------------------------

/// P3 — on a node without DNS, dialling a `/dns4/` address fails: the peer
/// behind the name is NOT reached (nothing resolves it — not the system, not a
/// third-party resolver: `localhost` would resolve anywhere, so any resolver
/// sneaked in would connect), and the plain reason is logged.
///
/// Control: the same dial connects when DNS is present
/// (`item12_p2_dns_control.rs`).
#[tokio::test(flavor = "current_thread")]
async fn p3_dns4_dial_without_dns_fails_with_the_plain_reason_and_is_not_resolved() {
    let (cap, _guard) = capture_logs();
    let a = spawn_with_source("item12-p3-A", [0x61; 32], [0x16; 32], false, no_system_dns).await;
    // B has DNS or not — irrelevant; it only has to be reachable by IP.
    let b = spawn_with_source("item12-p3-B", [0x62; 32], [0x16; 32], false, no_system_dns).await;

    let named = format!("/dns4/localhost/udp/{}/quic-v1/p2p/{}", udp_port(&b.loopback_addr), b.peer_id);
    let baseline = cap.count(PLAIN_REASON);
    a.cmd_tx.send(P2pCommand::Dial { address: named.clone() }).await.unwrap();

    assert!(
        log_grew(&cap, PLAIN_REASON, baseline, Duration::from_secs(5)).await,
        "P3: dialling {named} without DNS logs the plain reason ({PLAIN_REASON:?}).\nlogs:\n{}",
        cap.text()
    );
    // Leave time for any resolution path to finish connecting, then check none did.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        cap.count(&format!("Connected to {}", b.peer_id)),
        0,
        "P3: without DNS a name is not resolved by anyone, so B must not be reached via {named}.\nlogs:\n{}",
        cap.text()
    );
}

/// P3 — the same for `/dns6/`.
#[tokio::test(flavor = "current_thread")]
async fn p3_dns6_dial_without_dns_fails_with_the_plain_reason() {
    let (cap, _guard) = capture_logs();
    let a = spawn_with_source("item12-p3-dns6", [0x63; 32], [0x16; 32], false, no_system_dns).await;
    let target = keypair_from_seed(&[0x64; 32]).public().to_peer_id();
    let named = format!("/dns6/sovereign-item12.invalid/udp/4001/quic-v1/p2p/{target}");
    let baseline = cap.count(PLAIN_REASON);
    a.cmd_tx.send(P2pCommand::Dial { address: named.clone() }).await.unwrap();
    assert!(
        log_grew(&cap, PLAIN_REASON, baseline, Duration::from_secs(5)).await,
        "P3: dialling {named} without DNS logs the plain reason.\nlogs:\n{}",
        cap.text()
    );
}

/// P3 — logged once, not on every attempt: three dials of the same name give
/// one line with the reason.
// CONTRACT-QUESTION: "logged once" read as once per node (per process run of
// the node), across all named addresses; at minimum it must hold for repeated
// attempts at the SAME address, which is what this test asserts. Whether a
// second, different name logs again is not asserted. Also open: whether a
// startup line may carry the same words (allowed here, see the baseline).
#[tokio::test(flavor = "current_thread")]
async fn p3_reason_is_logged_once_across_repeated_attempts() {
    let (cap, _guard) = capture_logs();
    let a = spawn_with_source("item12-p3-once", [0x65; 32], [0x16; 32], false, no_system_dns).await;
    let target = keypair_from_seed(&[0x66; 32]).public().to_peer_id();
    let named = format!("/dns4/seed.sovereign-item12.invalid/udp/4001/quic-v1/p2p/{target}");
    let baseline = cap.count(PLAIN_REASON);

    for _ in 0..3 {
        a.cmd_tx.send(P2pCommand::Dial { address: named.clone() }).await.unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    // Let every attempt's failure surface before counting.
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert_eq!(
        cap.count(PLAIN_REASON) - baseline,
        1,
        "P3: the reason is logged once, not on every attempt (3 attempts at {named}).\nlogs:\n{}",
        cap.text()
    );
}
