//! rc2, Q2 of the laptop's second position (claude-coord
//! `from-laptop/0108-rc2-second-position.md`), settled by the coordinator
//! (2026-10-19): the salt file is written after `auth.store`, so a crash
//! between the two leaves `auth.store` without `salt`; at the next start
//! `prepare_auth` → `load_or_create_salt` used to mint a new random salt, and
//! from then on the pairing offer and the backup manifest carried a salt that
//! is not the account's. The rule: when `crypto/salt` is missing and
//! `crypto/auth.store` exists, the file is restored from the store's salt; a
//! new salt is minted only when neither exists.
//!
//! Written before the code, by another author: whoever implements it makes
//! these pass without editing this file. Registered from `setup.rs` (a child
//! module) because `load_or_create_salt(dir)` is private there; `dir` is the
//! crypto directory, as today — no signature change is assumed.
//! The atomic write (`write_private`) is checked by `rc2_second_position_tests`.

use super::load_or_create_salt;
use sovereign_crypto::auth::AuthStore;

const STORE_SALT: [u8; 32] = [0xA5; 32];

fn crypto_dir_with_store() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let crypto = root.path().join("crypto");
    std::fs::create_dir_all(&crypto).unwrap();
    let store = AuthStore::create(b"Primary!Pass1234", b"Duress!Pass5678", &STORE_SALT, "dev-salt").unwrap();
    store.save(&crypto.join("auth.store")).unwrap();
    (root, crypto)
}

#[test]
fn a_missing_salt_beside_an_auth_store_is_restored_from_the_store() {
    let (_root, crypto) = crypto_dir_with_store();
    assert!(!crypto.join("salt").exists());
    let store_salt = AuthStore::load(&crypto.join("auth.store")).unwrap().salt;
    assert_eq!(store_salt, STORE_SALT.to_vec(), "test setup");

    let salt = load_or_create_salt(&crypto).expect("load_or_create_salt");
    assert_eq!(salt, store_salt, "the returned salt must be the account's (auth.store's), not a new one");
    assert_eq!(
        std::fs::read(crypto.join("salt")).expect("the salt file is written back"),
        store_salt,
        "the salt file must be restored from auth.store's salt"
    );
    // And it stays put.
    assert_eq!(load_or_create_salt(&crypto).unwrap(), store_salt);
}

#[test]
fn with_neither_salt_nor_store_a_fresh_salt_is_minted_and_kept() {
    let root = tempfile::tempdir().unwrap();
    let crypto = root.path().join("crypto");
    std::fs::create_dir_all(&crypto).unwrap();

    let salt = load_or_create_salt(&crypto).expect("mint a salt");
    assert_eq!(salt.len(), 32, "a fresh salt is 32 bytes, as today");
    assert_ne!(salt, vec![0u8; 32], "a fresh salt is random");
    assert_eq!(std::fs::read(crypto.join("salt")).unwrap(), salt);
    assert_eq!(load_or_create_salt(&crypto).unwrap(), salt, "the second call reads the same salt");
    assert!(!crypto.join("auth.store").exists(), "minting a salt creates no auth.store");
}

#[test]
fn an_existing_salt_file_is_read_as_is() {
    let root = tempfile::tempdir().unwrap();
    let crypto = root.path().join("crypto");
    std::fs::create_dir_all(&crypto).unwrap();
    std::fs::write(crypto.join("salt"), [0x3C; 32]).unwrap();
    assert_eq!(load_or_create_salt(&crypto).unwrap(), vec![0x3C; 32]);
}
