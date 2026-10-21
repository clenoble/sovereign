//! rc2, Q2 of the laptop's second position (claude-coord
//! `from-laptop/0108-rc2-second-position.md`), the shell's half, settled by the
//! coordinator (2026-10-19): the shell gets the seam
//! `crate::crypto::load_or_create_salt_in(crypto_dir: &Path) -> anyhow::Result<Vec<u8>>`
//! with the app's rule — store present and salt missing → the file is restored
//! from the store's salt; neither → a salt is minted (as today: 32 random bytes
//! as 64 hex characters); an existing salt file → read as is; written through
//! `sovereign_crypto::fs_private::write_private`. `load_or_create_salt()`
//! becomes a wrapper over `crypto_dir()`. Mirrors
//! `sovereign-app/src/salt_restore_tests.rs`.
//!
//! Written before the code, by another author: whoever implements it makes
//! these pass without editing this file.

use crate::crypto::load_or_create_salt_in;
use crate::field_contrast_tests::{blank_line_comments, fn_body, read};
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

    let salt: Vec<u8> = load_or_create_salt_in(&crypto).expect("load_or_create_salt_in");
    assert_eq!(salt, store_salt, "the returned salt must be the account's (auth.store's), not a new one");
    assert_eq!(
        std::fs::read(crypto.join("salt")).expect("the salt file is written back"),
        store_salt,
        "the salt file must be restored from auth.store's salt"
    );
    assert_eq!(load_or_create_salt_in(&crypto).unwrap(), store_salt);
}

#[test]
fn with_neither_salt_nor_store_a_fresh_salt_is_minted_and_kept() {
    let root = tempfile::tempdir().unwrap();
    let crypto = root.path().join("crypto");
    std::fs::create_dir_all(&crypto).unwrap();

    let salt: Vec<u8> = load_or_create_salt_in(&crypto).expect("mint a salt");
    assert_eq!(salt.len(), 64, "a fresh shell salt is 64 hex characters, as today");
    assert!(salt.iter().all(|b: &u8| b.is_ascii_hexdigit()), "as today: hex text");
    assert_eq!(std::fs::read(crypto.join("salt")).unwrap(), salt);
    assert_eq!(load_or_create_salt_in(&crypto).unwrap(), salt, "the second call reads the same salt");
    assert!(!crypto.join("auth.store").exists(), "minting a salt creates no auth.store");
}

#[test]
fn an_existing_salt_file_is_read_as_is() {
    let root = tempfile::tempdir().unwrap();
    let crypto = root.path().join("crypto");
    std::fs::create_dir_all(&crypto).unwrap();
    std::fs::write(crypto.join("salt"), [0x3C; 32]).unwrap();
    assert_eq!(load_or_create_salt_in(&crypto).unwrap(), vec![0x3C; 32]);
}

/// `load_or_create_salt()` is the wrapper over `crypto_dir()`: one rule, one place.
#[test]
fn load_or_create_salt_goes_through_the_seam() {
    let code = blank_line_comments(&read("src/crypto.rs"));
    let body = fn_body(&code, "load_or_create_salt", "src/crypto.rs");
    assert!(
        body.contains("load_or_create_salt_in("),
        "src/crypto.rs `load_or_create_salt` must call `load_or_create_salt_in(` (on `crypto_dir()`), not repeat the rule:\n{body}"
    );
    let seam = fn_body(&code, "load_or_create_salt_in", "src/crypto.rs");
    assert!(
        seam.contains("write_private(") && !seam.contains("fs::write("),
        "src/crypto.rs `load_or_create_salt_in` must write the salt with `write_private(`, not `fs::write(`"
    );
}
