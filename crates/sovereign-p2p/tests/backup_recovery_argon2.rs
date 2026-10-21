//! rc2, Q3 of the laptop's second position (claude-coord
//! `from-laptop/0108-rc2-second-position.md`): does backup recovery recognise
//! the passphrase of an account made as onboarding makes it today?
//!
//! Onboarding creates the account with `AuthStore::create` (Argon2id,
//! `Kdf::current()`); the backup's owner tag comes from the session's
//! AccountKey (`sovereign-app/src/tauri_commands/backup.rs`:
//! `account_key.derive_backup_tag()`); `RecoveryState::passphrase_matches`
//! (`src/recovery.rs:298`) re-derives with `MasterKey::from_passphrase` (the
//! legacy HKDF). The laptop believes the two never meet ("read, not run"); this
//! test is the run. Ignored so it does not block the merge — a decision is
//! pending with Céline; run it with `-- --ignored`.
//!
//! A control (not ignored) checks the harness without taking a side on the
//! KDF: the recovery state built here reaches the tag comparison (no "no
//! manifest" / "no salt" error) and refuses a wrong passphrase. Measured on
//! 2026-10-19, `2e8e8ff`: the same harness recognises a legacy-HKDF account
//! (as `tests/e2e_backup.rs` builds it); that is not asserted, so the decision
//! (Argon2id only, or both) stays Céline's.
//!
//! Cost: one `AuthStore::create` (four Argon2id derivations at
//! 64 MiB / t=3, the only parameters the crate exposes) plus one
//! authentication — a few seconds in a debug build.

use base64::Engine;
use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::auth::AuthStore;
use sovereign_crypto::master_key::MasterKey;
use sovereign_p2p::backup::BackupManifest;
use sovereign_p2p::recovery::RecoveryState;

const PASSPHRASE: &[u8] = b"Correct!Horse9Battery";
const SALT: [u8; 32] = [0x5C; 32];

/// A recovery that has located the manifest and received the salt from a
/// guardian payload — the state in which the UI asks for the passphrase.
fn recovery_for(owner_tag: &str) -> RecoveryState {
    let manifest = BackupManifest::from_json(
        &serde_json::json!({
            "snapshot_id": "snap-q3",
            "epoch": 1,
            "created_at": "2026-10-19T00:00:00Z",
            "owner_tag": owner_tag,
            "ciphertext_digest": "",
            "ciphertext_len": 0,
            "nonce_b64": "",
            "data_fragments": 1,
            "parity_fragments": 0,
            "fragment_digests": [],
            "key_threshold": 2,
            "guardian_shards": []
        })
        .to_string(),
    )
    .expect("a manifest");
    let mut rec = RecoveryState::new("rec-q3".into(), owner_tag.to_string(), vec![], vec![]);
    rec.manifest = Some(manifest);
    rec.salt_b64 = Some(base64::engine::general_purpose::STANDARD.encode(SALT));
    rec
}

#[test]
fn control_the_harness_reaches_the_tag_comparison() {
    // Any well-formed tag: here a legacy-derived one, as e2e_backup.rs makes.
    let mk = MasterKey::from_passphrase(PASSPHRASE, &SALT).unwrap();
    let tag = AccountKey::derive(&mk).unwrap().derive_backup_tag();
    let rec = recovery_for(&tag);
    assert_eq!(
        rec.passphrase_matches(b"wrong passphrase").ok(),
        Some(false),
        "the harness must reach the comparison (manifest and salt in place) and refuse a wrong passphrase"
    );
}

#[test]
#[ignore = "backup recovery vs Argon2id accounts: decision pending (Céline), rc2 log"]
fn backup_recovery_recognises_the_passphrase_of_an_account_made_by_onboarding() {
    // The account as onboarding makes it (Kdf::current), and its session key.
    let store = AuthStore::create(PASSPHRASE, b"Duress!Pass5678", &SALT, "device-q3").unwrap();
    let session = store.authenticate(PASSPHRASE).unwrap();
    // The owner tag as the app's backup command derives it.
    let tag = session.account_key.derive_backup_tag();

    let rec = recovery_for(&tag);
    assert!(
        rec.passphrase_matches(PASSPHRASE).unwrap(),
        "backup recovery must recognise the right passphrase of an account created by AuthStore::create (Argon2id); \
         passphrase_matches re-derives with the legacy HKDF and gets another owner tag"
    );
}
