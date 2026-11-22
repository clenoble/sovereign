//! A join whose node did not start keeps the offer's peer and addresses for a
//! restart that succeeds (the implementer's own tests, lot LQ; C-SUP's are in
//! `csup_tests.rs`). The login itself needs a real account; these drive the
//! two places that take the join's peer, on a headless `App` with no node.

use std::path::PathBuf;
use std::sync::OnceLock;

use super::App;

fn ensure_test_data_dir() {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("SOVEREIGN_DATA_DIR") {
            return d.into();
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let d = std::env::temp_dir().join(format!("sov-join-stop-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

fn shell_with_a_pending_join() -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.locked = false;
    app.pending_join_peer = Some(("12D3KooWSource".into(), vec!["/ip4/192.0.2.7/udp/4001/quic-v1".into()]));
    app
}

// With no node, the join's first sync is not taken: its peer and addresses wait.
#[test]
fn with_no_node_the_join_s_peer_is_kept() {
    let mut app = shell_with_a_pending_join();
    assert!(app.p2p.is_none(), "precondition: no node");
    app.pair_sync_pending_join();
    assert!(app.pending_join_peer.is_some(), "the join's peer was dropped with no node to dial it");
}

// A restart that fails (no session keys here) keeps the join's peer for the next one.
#[test]
fn a_restart_that_fails_keeps_the_join_s_peer() {
    let mut app = shell_with_a_pending_join();
    app.sync_stopped = true;
    app.restart_sync();
    assert!(app.p2p.is_none(), "precondition: the restart failed");
    assert_eq!(
        app.pending_join_peer.as_ref().map(|(p, a)| (p.as_str(), a.len())),
        Some(("12D3KooWSource", 1)),
        "the join's peer and addresses were dropped by a failed restart"
    );
}
