//! rc2 — `auth.store` is created once (spec ch. 07, *Auth store*, "Created once,
//! never replaced by account creation", v2.0.46): "The first write of
//! `auth.store` refuses an existing file (an exclusive create, not a replace)."
//!
//! Written before the code, by another author: whoever implements
//! `AuthStore::save_new` makes these pass without editing this file.
//!
//! Contract under test: `AuthStore::save_new(&self, path: &Path) -> CryptoResult<()>`
//! writes the store only if `path` does not exist; if it exists, it returns an
//! error and the existing file stays byte-identical ("a refused request
//! changes no file").
//!
//! Cost: the crate's own tests build stores with `AuthStore::create` (Argon2id,
//! `Kdf::current()`); there is no cheaper public constructor, so each test
//! builds exactly one store and derives a second one by editing a loaded copy
//! (the fields are public), not by a second `create`.

use std::collections::BTreeSet;
use std::path::Path;

use sovereign_crypto::auth::{AuthStore, PersonaKind};

const PRIMARY: &[u8] = b"Primary!Pass1234";
const DURESS: &[u8] = b"Duress!Pass5678";
const SALT: &[u8] = b"test-salt-32-bytes-long-padding!";
const DEVICE: &str = "test-device-save-new";

fn new_store() -> AuthStore {
    AuthStore::create(PRIMARY, DURESS, SALT, DEVICE).expect("create a store")
}

fn dir_entries(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .expect("read the directory")
        .map(|e| e.expect("dir entry").file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn save_new_on_an_absent_path_writes_a_store_that_loads_and_authenticates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.store");
    assert!(!path.exists());

    let store = new_store();
    store
        .save_new(&path)
        .expect("save_new must write the store when the path does not exist");

    assert!(path.exists(), "save_new returned Ok but wrote no file");
    let loaded = AuthStore::load(&path).expect("the written store must load back");
    let primary = loaded
        .authenticate(PRIMARY)
        .expect("the loaded store must authenticate the primary passphrase");
    assert_eq!(primary.persona, PersonaKind::Primary);
    let duress = loaded
        .authenticate(DURESS)
        .expect("the loaded store must authenticate the duress passphrase");
    assert_eq!(duress.persona, PersonaKind::Duress);
    // The KEK survives the round trip: the loaded store unwraps the same KEK
    // as the store that was written.
    let original = store.authenticate(PRIMARY).unwrap();
    assert_eq!(
        original.kek.as_bytes(),
        primary.kek.as_bytes(),
        "the store read back must hold the KEK of the store that was written"
    );
}

#[test]
fn save_new_on_an_existing_file_errors_and_leaves_it_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.store");

    // The first account: written by save_new on an absent path.
    let first = new_store();
    first.save_new(&path).expect("first write on an absent path");
    let before = std::fs::read(&path).unwrap();
    let entries_before = dir_entries(dir.path());

    // A second, different store (as a second onboarding would produce): same
    // shape, other device id, so its serialization differs from the first.
    let mut second = AuthStore::load(&path).unwrap();
    second.device_id = "another-device".to_string();
    assert_ne!(
        serde_json::to_vec_pretty(&second).unwrap(),
        before,
        "test setup: the second store must serialize differently from the first"
    );

    let result = second.save_new(&path);
    assert!(
        result.is_err(),
        "save_new must refuse a path that already exists (exclusive create, not a replace)"
    );

    let after = std::fs::read(&path).unwrap();
    assert_eq!(
        after, before,
        "a refused save_new must leave the existing auth.store byte-identical"
    );
    assert_eq!(
        dir_entries(dir.path()),
        entries_before,
        "a refused save_new must change no file (no temp file left beside auth.store)"
    );
    // And the first account still opens.
    let ok = AuthStore::load(&path).unwrap().authenticate(PRIMARY).unwrap();
    assert_eq!(ok.persona, PersonaKind::Primary);
}

#[test]
fn save_new_refuses_any_existing_file_not_only_a_store() {
    // The refusal is about the path existing, whatever it holds: a file that is
    // not a valid store (e.g. a torn or foreign file) is not overwritten either.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.store");
    std::fs::write(&path, b"not a store").unwrap();

    let store = new_store();
    assert!(
        store.save_new(&path).is_err(),
        "save_new must refuse an existing file even if it is not a store"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"not a store");
}
