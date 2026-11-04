//! v0.0.12 lot 1, item 4 — P2c's follow-up (agreed by PANDA2, 2026-10-23):
//! with the source-device record written **before** `auth.store`
//! (`doc/plans/V0012-lot1.md`, P2c), a setup can stop after the record and
//! before the store (the claim refused, the store path unwritable). The user
//! then pairs again, with a new handshake: **the second record replaces the
//! first**; there is never a stale record of the same source beside it.
//!
//! It holds today because `PairingManager::add_device` inserts by `peer_id`
//! (`src/pairing.rs`); this file keeps it true. Written before the code, by
//! another author: the implementer does not edit it.
//!
//! What a second pairing changes: the source is the same (same `peer_id`);
//! the pair key differs (this device's final identity comes from the new
//! passphrase), and so may the store key of `paired_devices.json` (derived
//! from this device's key). Both ways step 7 may write the record are
//! covered: adding to the file it can read, or writing a fresh one.

use std::path::PathBuf;

use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::master_key::MasterKey;
use sovereign_p2p::pairing::{derive_store_key, PairedDevice, PairingManager};

const SOURCE: &str = "12D3KooWSourceDeviceOfThePairing";

fn temp_file(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("lot1-p2c-{}-{nanos}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("paired_devices.json")
}

fn store_key(seed: &str) -> [u8; 32] {
    let dk = DeviceKey::derive(&MasterKey::generate(), seed).unwrap();
    derive_store_key(&dk)
}

/// Step 7 of a pairing: the source's record with this pairing's pair key.
fn record(name: &str, key: u8) -> PairedDevice {
    PairedDevice::with_key(SOURCE.into(), name.into(), [key; 32])
}

fn only_record(m: &PairingManager) -> PairedDevice {
    let all = m.list_devices();
    assert_eq!(all.len(), 1, "one record of the source, never two: {:?}", all);
    all[0].clone()
}

// The same store key (the second pairing can read the first record): the
// record is loaded, the new one added, and it replaces the first.
#[test]
fn p2c_a_second_pairing_that_reads_the_first_record_replaces_it() {
    let path = temp_file("same-key");
    let key = store_key("same");
    let mut first = PairingManager::new(path.clone());
    first.add_device(record("Laptop", 1));
    first.save(&key).unwrap();
    // … auth.store fails; the user pairs again.
    let mut second = PairingManager::load(&path, &key).unwrap();
    second.add_device(record("Laptop (paired again)", 2));
    second.save(&key).unwrap();

    let r = only_record(&PairingManager::load(&path, &key).unwrap());
    assert_eq!(r.device_name, "Laptop (paired again)");
    assert_eq!(r.pair_key_b64, record("x", 2).pair_key_b64, "the second pairing's key");
}

// A new store key (a new passphrase gives this device a new key): the first
// file cannot be read; the second pairing writes its own, which is the only
// record, and the first key opens nothing any more.
#[test]
fn p2c_a_second_pairing_with_a_new_device_key_leaves_only_its_record() {
    let path = temp_file("new-key");
    let k1 = store_key("first");
    let k2 = store_key("second");
    let mut first = PairingManager::new(path.clone());
    first.add_device(record("Laptop", 1));
    first.save(&k1).unwrap();
    // … auth.store fails; the user pairs again with another passphrase.
    let mut second = PairingManager::load(&path, &k2).unwrap_or_else(|_| PairingManager::new(path.clone()));
    second.add_device(record("Laptop (paired again)", 2));
    second.save(&k2).unwrap();

    let r = only_record(&PairingManager::load(&path, &k2).unwrap());
    assert_eq!(r.pair_key_b64, record("x", 2).pair_key_b64);
    assert!(PairingManager::load(&path, &k1).is_err(), "the first pairing's store is gone");
}

// The rule itself: adding a record of a peer already present replaces it.
#[test]
fn p2c_add_device_replaces_a_record_of_the_same_peer() {
    let mut m = PairingManager::new(temp_file("rule"));
    m.add_device(record("first", 1));
    m.add_device(record("second", 2));
    let r = only_record(&m);
    assert_eq!(r.device_name, "second");
}
