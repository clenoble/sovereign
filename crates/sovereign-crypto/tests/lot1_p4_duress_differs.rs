//! v0.0.12 lot 1, item 4 — P4: duress differs from the main password, in the
//! store, on every path.
//!
//! Contract `doc/plans/V0012-lot1.md` (as amended in b35e0ff): "All three
//! constructors of `sovereign-crypto/src/auth.rs` (`create`,
//! `create_with_imported_account_key`, `create_with_secrets`) refuse a duress
//! passphrase equal to the primary one, **one test per constructor** (each
//! builds its own entries, so a check in `create` alone misses paired setup on
//! both platforms and guardian recovery)."
//!
//! Written before the code, by another author: whoever implements P4 makes
//! these pass **without editing this file**.
//!
//! What a refusal is: an `Err` whose text says it is about the duress
//! password (no error variant is named by the contract; the words the user
//! sees are `complete_onboarding`'s, tested in `sovereign-app`). Equality is
//! byte equality of the passphrases as given. Each test also checks that a
//! different duress passphrase is still accepted, so the check is not "refuse
//! everything".
//!
//! Cost: each accepted store is one Argon2id pair (`Kdf::current()`), so each
//! test builds exactly one accepted store.

use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::auth::AuthStore;
use sovereign_crypto::kek::Kek;

const SAME: &[u8] = b"Same!Pass1234";
const OTHER: &[u8] = b"Other!Pass5678";
const SALT: &[u8] = b"test-salt-32-bytes-long-padding!";
const DEVICE: &str = "test-device-p4";

fn assert_refused(r: Result<AuthStore, sovereign_crypto::error::CryptoError>, ctor: &str) {
    match r {
        Ok(_) => panic!("{ctor}: a duress passphrase equal to the primary one was accepted"),
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            assert!(msg.contains("duress"), "{ctor}: the refusal says it is about the duress password: {e}");
        }
    }
}

#[test]
fn p4_create_refuses_a_duress_passphrase_equal_to_the_primary() {
    assert_refused(AuthStore::create(SAME, SAME, SALT, DEVICE), "create");
    AuthStore::create(SAME, OTHER, SALT, DEVICE).expect("create: different passphrases are accepted");
}

#[test]
fn p4_create_with_imported_account_key_refuses_a_duress_passphrase_equal_to_the_primary() {
    let ak = AccountKey::from_bytes([7u8; 32]);
    assert_refused(
        AuthStore::create_with_imported_account_key(SAME, SAME, SALT, DEVICE, &ak),
        "create_with_imported_account_key (paired setup)",
    );
    AuthStore::create_with_imported_account_key(SAME, OTHER, SALT, DEVICE, &ak)
        .expect("create_with_imported_account_key: different passphrases are accepted");
}

#[test]
fn p4_create_with_secrets_refuses_a_duress_passphrase_equal_to_the_primary() {
    let ak = AccountKey::from_bytes([9u8; 32]);
    let kek = Kek::generate();
    assert_refused(
        AuthStore::create_with_secrets(SAME, SAME, SALT, DEVICE, &ak, &kek),
        "create_with_secrets (guardian recovery)",
    );
    AuthStore::create_with_secrets(SAME, OTHER, SALT, DEVICE, &ak, &kek)
        .expect("create_with_secrets: different passphrases are accepted");
}

// The comparison is exact: a duress passphrase that differs only in case or by
// a trailing space is a different passphrase, and is accepted (the wizard
// trims what it sends; the store compares what it is given).
#[test]
fn p4_the_comparison_is_on_the_exact_bytes() {
    AuthStore::create(b"Same!Pass1234", b"same!pass1234", SALT, DEVICE).expect("differs in case: accepted");
}
