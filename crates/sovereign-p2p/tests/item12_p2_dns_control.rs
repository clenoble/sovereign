//! v0.0.12 item 12 — P2, the half that must stay as it is: where the system
//! resolver config exists (this desktop), `SovereignNode::new` builds the DNS
//! transport as today, so a peer given by name is dialled.
//!
//! Uses only today's API, so it runs before the item is built. It is also the
//! control for P3's "a node without DNS does not connect through a name": the
//! same `/dns4/localhost/...` dial that must FAIL without DNS succeeds here.
//! `localhost` is answered by the resolver itself (RFC 6761, hickory's early
//! return), so no network and no name server is involved.
//!
//! Runs on the system resolver config of the machine: on a host without one
//! (Android, a container with no /etc/resolv.conf) it cannot pass — that is
//! precisely the phone's case and is covered by the injected-source tests.

use std::time::Duration;

mod item12_common;
use item12_common::*;

use sovereign_p2p::{P2pCommand, SovereignNode};

/// Build with today's constructor and start.
async fn spawn(device: &str, seed: [u8; 32]) -> Harness {
    let mut p = parts(device, seed, [0x12; 32], false).await;
    let peer_id = p.kp.public().to_peer_id();
    let cmd_rx = p.cmd_rx.take().unwrap();
    let node = SovereignNode::new(&p.cfg, p.kp.clone(), p.event_tx.clone(), cmd_rx, p.svc.clone(), None)
        .expect("node builds on a host with a system resolver config");
    start(node, p, peer_id, true).await
}

/// P2 (control) — with the system resolver config present, a `/dns4/` name is
/// resolved and the peer behind it is reached.
#[tokio::test(flavor = "current_thread")]
async fn p2_with_system_dns_a_named_peer_is_dialled_as_today() {
    let (cap, _guard) = capture_logs();
    let a = spawn("item12-ctl-A", [0x21; 32]).await;
    let b = spawn("item12-ctl-B", [0x22; 32]).await;

    let named = format!("/dns4/localhost/udp/{}/quic-v1/p2p/{}", udp_port(&b.loopback_addr), b.peer_id);
    a.cmd_tx.send(P2pCommand::Dial { address: named.clone() }).await.unwrap();

    // A logs "Connected to <B>" (node.rs, ConnectionEstablished).
    let connected = log_seen(&cap, &format!("Connected to {}", b.peer_id), Duration::from_secs(10)).await;
    assert!(
        connected,
        "with the system DNS config, dialling {named} must reach B as today.\nlogs:\n{}",
        cap.text()
    );
}
