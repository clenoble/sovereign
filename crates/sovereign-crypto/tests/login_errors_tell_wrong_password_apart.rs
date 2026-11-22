//! Login errors say what failed: the core tells a wrong passphrase apart
//! (spec ch. 07, « Login errors say what failed — contract (LE) », LE1 and
//! LE5). Specified 2026-11-19 (Céline, through the Fable maille).
//!
//! Written before the code, by another author: whoever implements the
//! contract makes these pass without editing this file.
//!
//! What is held:
//! - a wrong passphrase (the derivation succeeded, no probe opened) returns
//!   `CryptoError::WrongPassphrase`, for a store with a primary and a duress
//!   entry;
//! - the right passphrase, primary or duress, whose entry's wrapped KEK or
//!   wrapped account key is damaged, returns another error (an unwrap that
//!   fails after a probe opened);
//! - a store whose key-derivation parameters the derivation refuses returns
//!   another error, `DerivationFailed`.
//!
//! Which on-disk entry belongs to which persona is not known from outside
//! (their order is random): it is found by damaging one entry and seeing
//! whether the passphrase still opens the store.
//!
//! Cost: each store is one `AuthStore::create` (two Argon2id derivations at
//! the production parameters), each authentication one more.

use sovereign_crypto::auth::AuthStore;
use sovereign_crypto::master_key::Kdf;
use sovereign_crypto::CryptoError;

const PRIMARY: &[u8] = b"Primary!Pass1234";
const DURESS: &[u8] = b"Duress!Pass5678";
const WRONG: &[u8] = b"Wrong!Pass9012";
const SALT: &[u8] = b"test-salt-32-bytes-long-padding!";
const DEVICE: &str = "test-device-login-errors";

fn new_store() -> AuthStore {
    AuthStore::create(PRIMARY, DURESS, SALT, DEVICE).expect("a store with a primary and a duress entry")
}

/// A copy of the store through its own save and load, so one entry can be
/// damaged while the original stays whole.
fn copy_of(store: &AuthStore) -> AuthStore {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("auth.store");
    store.save(&path).expect("save");
    AuthStore::load(&path).expect("load")
}

fn flip(bytes: &mut [u8]) {
    assert!(!bytes.is_empty(), "nothing to damage");
    for b in bytes.iter_mut() {
        *b ^= 0xA5;
    }
}

/// The index of the entry that `passphrase` opens: damaging that entry's
/// wrapped KEK makes the store refuse the passphrase; damaging the other
/// entry's leaves it opening.
fn entry_opened_by(store: &AuthStore, passphrase: &[u8]) -> usize {
    assert!(store.authenticate(passphrase).is_ok(), "the whole store opens with this passphrase");
    let mut damaged = copy_of(store);
    flip(&mut damaged.personas[0].wrapped_kek.ciphertext);
    if damaged.authenticate(passphrase).is_ok() {
        1
    } else {
        0
    }
}

fn assert_wrong_passphrase(r: Result<impl Sized, CryptoError>, what: &str) {
    match r {
        Ok(_) => panic!("{what}: the store opened"),
        Err(CryptoError::WrongPassphrase) => {}
        Err(e) => panic!("{what}: expected the wrong-passphrase error, got « {e} » ({e:?})"),
    }
}

fn assert_not_wrong_passphrase(r: Result<impl Sized, CryptoError>, what: &str) -> CryptoError {
    match r {
        Ok(_) => panic!("{what}: the store opened although it was damaged"),
        Err(CryptoError::WrongPassphrase) => {
            panic!("{what}: a failure after the probe opened read as a wrong passphrase")
        }
        Err(e) => e,
    }
}

// LE1: a wrong passphrase — the derivation succeeded and no probe opened.
#[test]
fn a_wrong_passphrase_returns_the_wrong_passphrase_error() {
    let store = new_store();
    assert_wrong_passphrase(store.authenticate(WRONG), "a wrong passphrase");
    // The empty passphrase derives as well; nothing opens.
    assert_wrong_passphrase(store.authenticate(b""), "an empty passphrase");
}

// LE1 and LE5: the primary's probe opened, its KEK will not unwrap.
#[test]
fn the_right_primary_passphrase_with_its_kek_damaged_is_not_a_wrong_passphrase() {
    let store = new_store();
    let i = entry_opened_by(&store, PRIMARY);
    let mut damaged = copy_of(&store);
    flip(&mut damaged.personas[i].wrapped_kek.ciphertext);
    assert_not_wrong_passphrase(damaged.authenticate(PRIMARY), "primary, KEK damaged");
    // The other passphrase still opens its own entry, untouched.
    assert!(damaged.authenticate(DURESS).is_ok(), "the duress entry was not damaged");
}

// LE5: the same for the duress persona.
#[test]
fn the_right_duress_passphrase_with_its_kek_damaged_is_not_a_wrong_passphrase() {
    let store = new_store();
    let i = entry_opened_by(&store, DURESS);
    let mut damaged = copy_of(&store);
    flip(&mut damaged.personas[i].wrapped_kek.ciphertext);
    assert_not_wrong_passphrase(damaged.authenticate(DURESS), "duress, KEK damaged");
    assert!(damaged.authenticate(PRIMARY).is_ok(), "the primary entry was not damaged");
}

// LE1 and LE5: the AccountKey's unwrap fails after the probe opened, for each persona.
#[test]
fn the_right_passphrase_with_its_account_key_damaged_is_not_a_wrong_passphrase() {
    let store = new_store();
    for (name, pass) in [("primary", PRIMARY), ("duress", DURESS)] {
        let i = entry_opened_by(&store, pass);
        let mut damaged = copy_of(&store);
        let wrapped = damaged.personas[i]
            .wrapped_account_key
            .as_mut()
            .expect("a new store wraps the account key in each entry");
        flip(&mut wrapped.ciphertext);
        assert_not_wrong_passphrase(damaged.authenticate(pass), &format!("{name}, account key damaged"));
    }
}

// LE1 and LE5: the two personas' wrong passphrase is the same error. A wrong
// passphrase cannot say which persona it was not.
#[test]
fn a_wrong_passphrase_reads_the_same_for_a_store_whose_entries_are_in_either_order() {
    let store = new_store();
    let mut swapped = copy_of(&store);
    swapped.personas.swap(0, 1);
    assert_wrong_passphrase(store.authenticate(WRONG), "a wrong passphrase");
    assert_wrong_passphrase(swapped.authenticate(WRONG), "a wrong passphrase, entries swapped");
}

// LE1: a derivation that fails returns another error, which carries its cause.
#[test]
fn a_derivation_the_parameters_refuse_is_not_a_wrong_passphrase() {
    let store = new_store();
    let mut refused = copy_of(&store);
    refused.kdf = Kdf::Argon2id {
        m_cost_kib: 0,
        t_cost: 0,
        p_cost: 0,
    };
    let e = assert_not_wrong_passphrase(refused.authenticate(PRIMARY), "derivation refused");
    match e {
        CryptoError::DerivationFailed(cause) => {
            assert!(!cause.trim().is_empty(), "the derivation's error carries its cause")
        }
        other => panic!("derivation refused: expected DerivationFailed, got « {other} » ({other:?})"),
    }
    // And for a wrong passphrase too: no derivation, so nothing says « wrong ».
    let e = assert_not_wrong_passphrase(refused.authenticate(WRONG), "derivation refused, wrong passphrase");
    assert!(matches!(e, CryptoError::DerivationFailed(_)), "got « {e} » ({e:?})");
}
