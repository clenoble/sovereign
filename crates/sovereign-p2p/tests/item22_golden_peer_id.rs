//! v0.0.12 item 22 — a golden vector for the device identity (PANDA2's second
//! position on item 22, suggestion 1).
//!
//! A fixed device key and the exact peer id **v0.0.11's**
//! `sovereign_p2p::identity::derive_keypair` gave for it. Item 22 moved the
//! derivation into `sovereign_crypto::peer_identity` (the one every host
//! reaches); both it and the node's derivation must still give v0.0.11's
//! string. A change that moves every device's peer id (the HKDF info, the
//! seed handling, the key type, a libp2p upgrade that encodes differently)
//! breaks every pairing and the identity of every stamp written since item
//! 22: this test fails first.
//!
//! **How the string was computed** (2026-10-24, by the test author): a
//! throwaway worktree at tag `v0.0.11` (commit `60d4cee`), never committed and
//! removed after, with an example in `crates/sovereign-p2p/examples/golden.rs`
//! running exactly the steps of [`golden_device_key`] then
//! `sovereign_p2p::identity::derive_keypair(&dk)?.public().to_peer_id()`;
//! `cargo run -p sovereign-p2p --example golden` printed
//! `device_key_bytes=f0be9b91…06c0` and
//! `peer_id=12D3KooWLSmSjFkWyRnCgEWYFaXhVWBB191zBJN5JndjTd6Bobnu`.
//!
//! Written before any code that would need it, by another author: whoever
//! changes the derivation makes this pass **without editing this file**, or
//! reports why every device's id must move (a decision for Céline: pairings
//! would have to be redone).

use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::master_key::MasterKey;

/// The device key's bytes, as v0.0.11 derived them from [`golden_device_key`]'s
/// inputs.
const GOLDEN_DEVICE_KEY: &str = "f0be9b91e05c7f38843d02c91c58615ddcdc5c463b8153dad6e49f69651106c0";

/// The peer id v0.0.11's `derive_keypair` gave for that key.
const GOLDEN_PEER_ID: &str = "12D3KooWLSmSjFkWyRnCgEWYFaXhVWBB191zBJN5JndjTd6Bobnu";

/// The fixed device key: a master key of fixed bytes, then the device key for
/// a fixed device id (HKDF only, no passphrase stretching). `DeviceKey` has no
/// constructor from bytes; its bytes are checked against v0.0.11's below.
fn golden_device_key() -> DeviceKey {
    let mk = MasterKey::from_bytes([0x5a; 32]);
    DeviceKey::derive(&mk, "item22-golden").expect("derive the device key")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// The precondition of the vector: the fixed key is the same bytes v0.0.11
// had (so a failure below is the peer-id derivation, not the key's).
#[test]
fn item22_golden_the_fixed_device_key_has_v0011s_bytes() {
    assert_eq!(hex(golden_device_key().as_bytes()), GOLDEN_DEVICE_KEY, "the device key derivation moved");
}

// `sovereign_crypto::peer_identity::peer_id` (what a document service stamps
// with since item 22) gives v0.0.11's peer id.
#[test]
fn item22_golden_peer_identity_gives_v0011s_peer_id() {
    let dk = golden_device_key();
    assert_eq!(
        sovereign_crypto::peer_identity::peer_id(&dk).expect("derive"),
        GOLDEN_PEER_ID,
        "sovereign_crypto::peer_identity::peer_id moved every device's id"
    );
    assert!(sovereign_crypto::peer_identity::is_peer_id(GOLDEN_PEER_ID), "the golden string is a peer id");
}

// The node's derivation (`sovereign_p2p::identity::derive_keypair`, what the
// P2P node runs under) gives v0.0.11's peer id too — the same as the stamps'.
#[test]
fn item22_golden_the_nodes_derivation_gives_v0011s_peer_id() {
    let dk = golden_device_key();
    let node = sovereign_p2p::identity::derive_keypair(&dk).expect("derive").public().to_peer_id().to_string();
    assert_eq!(node, GOLDEN_PEER_ID, "the node's derivation moved every device's id");
    assert_eq!(
        sovereign_crypto::peer_identity::derive_keypair(&dk).expect("derive").public().to_peer_id().to_string(),
        GOLDEN_PEER_ID,
        "sovereign_crypto::peer_identity::derive_keypair moved every device's id"
    );
}
