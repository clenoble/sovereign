use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::account_key::{AccountKey, WrappedAccountKey};
use crate::aead::{self, NONCE_SIZE};
use crate::device_key::DeviceKey;
use crate::error::{CryptoError, CryptoResult};
use crate::kek::{Kek, WrappedKek};
use crate::master_key::{Kdf, MasterKey};

/// Tagged probe plaintexts — embedded in each persona entry so we can
/// identify which persona a passphrase unlocks after decryption.
/// An attacker who cracks one passphrase already knows there are exactly
/// 2 entries, so tagging leaks nothing additional.
const PRIMARY_PROBE: &[u8] = b"sovereign-auth-probe-v1-primary";
const DURESS_PROBE: &[u8] = b"sovereign-auth-probe-v1-duress";

// ── Password policy ──────────────────────────────────────────────────

/// Requirements for password complexity.
#[derive(Debug, Clone)]
pub struct PasswordPolicy {
    pub min_length: usize,
    pub max_length: usize,
    pub require_uppercase: bool,
    pub require_lowercase: bool,
    pub require_digit: bool,
    pub require_special: bool,
}

/// Result of validating a password against a policy.
#[derive(Debug, Clone)]
pub struct PasswordValidation {
    pub valid: bool,
    pub errors: Vec<String>,
}

impl PasswordPolicy {
    pub fn default_policy() -> Self {
        Self {
            min_length: 12,
            max_length: 128,
            require_uppercase: true,
            require_lowercase: true,
            require_digit: true,
            require_special: true,
        }
    }

    pub fn validate(&self, password: &str) -> PasswordValidation {
        let mut errors = Vec::new();
        if password.len() < self.min_length {
            errors.push(format!("At least {} characters", self.min_length));
        }
        if password.len() > self.max_length {
            errors.push(format!("At most {} characters", self.max_length));
        }
        if self.require_uppercase && !password.chars().any(|c| c.is_uppercase()) {
            errors.push("At least one uppercase letter".into());
        }
        if self.require_lowercase && !password.chars().any(|c| c.is_lowercase()) {
            errors.push("At least one lowercase letter".into());
        }
        if self.require_digit && !password.chars().any(|c| c.is_ascii_digit()) {
            errors.push("At least one digit".into());
        }
        if self.require_special
            && !password
                .chars()
                .any(|c| !c.is_alphanumeric() && !c.is_whitespace())
        {
            errors.push("At least one special character".into());
        }
        PasswordValidation {
            valid: errors.is_empty(),
            errors,
        }
    }
}

// ── Persona ──────────────────────────────────────────────────────────

/// Which persona a passphrase unlocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonaKind {
    Primary,
    Duress,
}

// ── AuthStore ────────────────────────────────────────────────────────

/// Persisted authentication data. Contains two persona entries in randomized
/// order — indistinguishable without a passphrase.
#[derive(Serialize, Deserialize)]
pub struct AuthStore {
    pub salt: Vec<u8>,
    pub device_id: String,
    /// KDF used to stretch the passphrase into the MasterKey. Recorded so
    /// the store always unlocks with the KDF it was created under. Stores
    /// written before this field existed (v<=0.0.6, all HKDF) deserialize
    /// to [`Kdf::LegacyHkdf`] via the serde default and still open; new
    /// stores use [`Kdf::current`] (Argon2id). See CRYPTO-001.
    #[serde(default = "Kdf::legacy")]
    pub kdf: Kdf,
    /// Always exactly 2 entries, randomized order.
    pub personas: Vec<PersonaEntry>,
}

/// One persona: a probe ciphertext + a wrapped KEK.
/// The probe is a tagged plaintext encrypted with this persona's DeviceKey.
/// Successful decryption proves the passphrase matches and reveals the persona kind.
#[derive(Serialize, Deserialize)]
pub struct PersonaEntry {
    pub probe_ciphertext: Vec<u8>,
    pub probe_nonce: [u8; NONCE_SIZE],
    pub wrapped_kek: WrappedKek,
    /// AccountKey wrapped under this persona's DeviceKey. Optional for
    /// backwards compatibility with v0.0.4 stores: when missing, the
    /// AccountKey is derived from the MasterKey at authenticate-time.
    /// When present (paired devices, fresh v0.0.5 installs), the stored
    /// value wins so an imported AccountKey survives across restarts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrapped_account_key: Option<WrappedAccountKey>,
    /// Random opaque label — NOT "primary" / "duress".
    pub label: [u8; 16],
}

/// Successful authentication result.
pub struct AuthSuccess {
    pub persona: PersonaKind,
    /// Per-device key used for libp2p identity and KEK wrapping.
    pub device_key: DeviceKey,
    /// User-scoped at-rest key used for vault, body_raw, session log.
    /// Same on every device that shares this MasterKey (paired devices).
    pub account_key: AccountKey,
    pub kek: Kek,
}

impl AuthStore {
    /// Create a new AuthStore with primary + duress personas.
    pub fn create(
        primary_passphrase: &[u8],
        duress_passphrase: &[u8],
        salt: &[u8],
        device_id: &str,
    ) -> CryptoResult<Self> {
        let kdf = Kdf::current();
        let primary_entry =
            Self::build_entry(primary_passphrase, salt, device_id, PRIMARY_PROBE, &kdf)?;
        let duress_entry =
            Self::build_entry(duress_passphrase, salt, device_id, DURESS_PROBE, &kdf)?;

        // Randomize order so file inspection can't correlate position with kind.
        let mut personas = vec![primary_entry, duress_entry];
        if rand::rng().random_bool(0.5) {
            personas.swap(0, 1);
        }

        Ok(Self {
            salt: salt.to_vec(),
            device_id: device_id.to_string(),
            kdf,
            personas,
        })
    }

    /// Try to authenticate with a passphrase.
    /// Derives keys, tries each persona probe. Returns the matching persona,
    /// DeviceKey, and unwrapped KEK on success.
    pub fn authenticate(&self, passphrase: &[u8]) -> CryptoResult<AuthSuccess> {
        let master = MasterKey::derive(passphrase, &self.salt, &self.kdf)?;
        let device_key = DeviceKey::derive(&master, &self.device_id)?;
        // Fallback AccountKey: user-scoped, derived from MasterKey alone.
        // Used when the persona has no `wrapped_account_key` (v0.0.4
        // stores). When present, the stored AccountKey wins so paired
        // devices keep their imported key even though the local passphrase
        // would derive a different one.
        let derived_account_key = AccountKey::derive(&master)?;

        for entry in &self.personas {
            if let Ok(plaintext) = aead::decrypt(
                &entry.probe_ciphertext,
                &entry.probe_nonce,
                device_key.as_bytes(),
            ) {
                let persona = if plaintext == PRIMARY_PROBE {
                    PersonaKind::Primary
                } else if plaintext == DURESS_PROBE {
                    PersonaKind::Duress
                } else {
                    continue;
                };
                let kek = Kek::unwrap(&entry.wrapped_kek, &device_key)?;
                let account_key = match &entry.wrapped_account_key {
                    Some(wrapped) => AccountKey::unwrap_with(wrapped, &device_key)?,
                    None => AccountKey::derive(&master)?,
                };
                let _ = &derived_account_key; // suppress unused warning when wrapped path is taken
                return Ok(AuthSuccess {
                    persona,
                    device_key,
                    account_key,
                    kek,
                });
            }
        }

        Err(CryptoError::DecryptionFailed)
    }

    /// Save to disk as JSON.
    pub fn save(&self, path: &std::path::Path) -> CryptoResult<()> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|e| CryptoError::Serialization(e.to_string()))?;
        crate::fs_private::write_private(path, json).map_err(|e| CryptoError::KeyDbIo(e.to_string()))?;
        Ok(())
    }

    /// Load from disk.
    pub fn load(path: &std::path::Path) -> CryptoResult<Self> {
        let bytes = std::fs::read(path).map_err(|e| CryptoError::KeyDbIo(e.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|e| CryptoError::Serialization(e.to_string()))
    }

    fn build_entry(
        passphrase: &[u8],
        salt: &[u8],
        device_id: &str,
        probe_plaintext: &[u8],
        kdf: &Kdf,
    ) -> CryptoResult<PersonaEntry> {
        // Default new-store entry derives the AccountKey locally from
        // the MasterKey. Pairing flows pass an imported AccountKey via
        // `build_entry_with_account_key`.
        let master = MasterKey::derive(passphrase, salt, kdf)?;
        let account_key = AccountKey::derive(&master)?;
        Self::build_entry_with_account_key(
            passphrase,
            salt,
            device_id,
            probe_plaintext,
            &account_key,
            kdf,
        )
    }

    fn build_entry_with_account_key(
        passphrase: &[u8],
        salt: &[u8],
        device_id: &str,
        probe_plaintext: &[u8],
        account_key: &AccountKey,
        kdf: &Kdf,
    ) -> CryptoResult<PersonaEntry> {
        let master = MasterKey::derive(passphrase, salt, kdf)?;
        let device_key = DeviceKey::derive(&master, device_id)?;
        let kek = Kek::generate();
        let wrapped_kek = kek.wrap(&device_key)?;
        let wrapped_account_key = account_key.wrap(&device_key)?;
        let (probe_ciphertext, probe_nonce) =
            aead::encrypt(probe_plaintext, device_key.as_bytes())?;
        let mut label = [0u8; 16];
        rand::rng().fill_bytes(&mut label);

        Ok(PersonaEntry {
            probe_ciphertext,
            probe_nonce,
            wrapped_kek,
            wrapped_account_key: Some(wrapped_account_key),
            label,
        })
    }

    /// Create an AuthStore where the **primary** persona's AccountKey is
    /// taken from `imported_account_key` rather than derived from the
    /// primary passphrase. Used by the pairing flow on the new device:
    /// the AccountKey arrives via QR + PIN out-of-band, and the local
    /// passphrase only protects the wrapping (not the AccountKey itself).
    ///
    /// The duress persona's AccountKey is derived locally from the
    /// duress passphrase as usual — duress workspaces are per-device
    /// and not synced.
    pub fn create_with_imported_account_key(
        primary_passphrase: &[u8],
        duress_passphrase: &[u8],
        salt: &[u8],
        device_id: &str,
        imported_account_key: &AccountKey,
    ) -> CryptoResult<Self> {
        let kdf = Kdf::current();
        let primary_entry = Self::build_entry_with_account_key(
            primary_passphrase,
            salt,
            device_id,
            PRIMARY_PROBE,
            imported_account_key,
            &kdf,
        )?;
        let duress_entry =
            Self::build_entry(duress_passphrase, salt, device_id, DURESS_PROBE, &kdf)?;

        let mut personas = vec![primary_entry, duress_entry];
        if rand::rng().random_bool(0.5) {
            personas.swap(0, 1);
        }

        Ok(Self {
            salt: salt.to_vec(),
            device_id: device_id.to_string(),
            kdf,
            personas,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SALT: &[u8] = b"test-salt-32-bytes-long-padding!";
    const TEST_DEVICE: &str = "test-device-001";

    #[test]
    fn password_policy_accepts_strong() {
        let policy = PasswordPolicy::default_policy();
        let result = policy.validate("MyStr0ng!Pass");
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn password_policy_rejects_short() {
        let policy = PasswordPolicy::default_policy();
        let result = policy.validate("Ab1!");
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("12")));
    }

    #[test]
    fn password_policy_rejects_no_uppercase() {
        let policy = PasswordPolicy::default_policy();
        let result = policy.validate("mystrongpass1!");
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("uppercase")));
    }

    #[test]
    fn password_policy_rejects_no_digit() {
        let policy = PasswordPolicy::default_policy();
        let result = policy.validate("MyStrongPass!x");
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("digit")));
    }

    #[test]
    fn password_policy_rejects_no_special() {
        let policy = PasswordPolicy::default_policy();
        let result = policy.validate("MyStrongPass1x");
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.contains("special")));
    }

    #[test]
    fn auth_store_primary_roundtrip() {
        let store = AuthStore::create(
            b"Primary!Pass1234",
            b"Duress!Pass5678",
            TEST_SALT,
            TEST_DEVICE,
        )
        .unwrap();

        let result = store.authenticate(b"Primary!Pass1234").unwrap();
        assert_eq!(result.persona, PersonaKind::Primary);
    }

    #[test]
    fn auth_store_duress_roundtrip() {
        let store = AuthStore::create(
            b"Primary!Pass1234",
            b"Duress!Pass5678",
            TEST_SALT,
            TEST_DEVICE,
        )
        .unwrap();

        let result = store.authenticate(b"Duress!Pass5678").unwrap();
        assert_eq!(result.persona, PersonaKind::Duress);
    }

    #[test]
    fn auth_store_wrong_password_fails() {
        let store = AuthStore::create(
            b"Primary!Pass1234",
            b"Duress!Pass5678",
            TEST_SALT,
            TEST_DEVICE,
        )
        .unwrap();

        assert!(store.authenticate(b"WrongPassword99!").is_err());
    }

    #[test]
    fn auth_store_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.store");

        let store = AuthStore::create(
            b"Primary!Pass1234",
            b"Duress!Pass5678",
            TEST_SALT,
            TEST_DEVICE,
        )
        .unwrap();
        store.save(&path).unwrap();

        let loaded = AuthStore::load(&path).unwrap();
        let primary = loaded.authenticate(b"Primary!Pass1234").unwrap();
        assert_eq!(primary.persona, PersonaKind::Primary);

        let duress = loaded.authenticate(b"Duress!Pass5678").unwrap();
        assert_eq!(duress.persona, PersonaKind::Duress);
    }

    #[test]
    fn auth_store_always_two_personas() {
        let store = AuthStore::create(
            b"Primary!Pass1234",
            b"Duress!Pass5678",
            TEST_SALT,
            TEST_DEVICE,
        )
        .unwrap();
        assert_eq!(store.personas.len(), 2);
    }

    #[test]
    fn persona_labels_are_random() {
        let store = AuthStore::create(
            b"Primary!Pass1234",
            b"Duress!Pass5678",
            TEST_SALT,
            TEST_DEVICE,
        )
        .unwrap();
        assert_ne!(store.personas[0].label, store.personas[1].label);
    }

    #[test]
    fn auth_store_creates_wrapped_account_key_for_each_persona() {
        let store = AuthStore::create(
            b"Primary!Pass1234",
            b"Duress!Pass5678",
            TEST_SALT,
            TEST_DEVICE,
        )
        .unwrap();
        // Both personas must have the wrapped_account_key field populated
        // post-v0.0.5 so paired-device imports survive a save/load cycle.
        assert!(store.personas.iter().all(|p| p.wrapped_account_key.is_some()));
    }

    #[test]
    fn imported_account_key_survives_authenticate() {
        // Pairing flow: device A's AccountKey is imported on device B.
        // After authenticate on B, AuthSuccess.account_key must equal
        // the imported value byte-for-byte, NOT the locally-derived one.
        let mk_imported = MasterKey::from_passphrase(b"shared", b"shared-salt").unwrap();
        let imported_ak = AccountKey::derive(&mk_imported).unwrap();

        let store = AuthStore::create_with_imported_account_key(
            b"NewDevicePass1!",
            b"NewDuressPass2!",
            TEST_SALT,
            TEST_DEVICE,
            &imported_ak,
        )
        .unwrap();

        let primary = store.authenticate(b"NewDevicePass1!").unwrap();
        assert_eq!(primary.persona, PersonaKind::Primary);
        assert_eq!(
            primary.account_key.as_bytes(),
            imported_ak.as_bytes(),
            "Primary AccountKey on the paired device must be the imported value"
        );

        // Duress is still derived locally from the new device's
        // duress passphrase — different MasterKey → different AccountKey.
        let duress = store.authenticate(b"NewDuressPass2!").unwrap();
        assert_eq!(duress.persona, PersonaKind::Duress);
        assert_ne!(
            duress.account_key.as_bytes(),
            imported_ak.as_bytes(),
            "Duress AccountKey is per-device, not the imported one"
        );
    }

    #[test]
    fn imported_account_key_survives_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.store");
        let mk_imported = MasterKey::from_passphrase(b"shared", b"shared-salt").unwrap();
        let imported_ak = AccountKey::derive(&mk_imported).unwrap();

        let store = AuthStore::create_with_imported_account_key(
            b"NewDevicePass1!",
            b"NewDuressPass2!",
            TEST_SALT,
            TEST_DEVICE,
            &imported_ak,
        )
        .unwrap();
        store.save(&path).unwrap();

        let loaded = AuthStore::load(&path).unwrap();
        let primary = loaded.authenticate(b"NewDevicePass1!").unwrap();
        assert_eq!(primary.account_key.as_bytes(), imported_ak.as_bytes());
    }

    #[test]
    fn legacy_v04_store_without_wrapped_account_key_still_works() {
        // Simulate a TRUE v0.0.4 file: HKDF KDF (LegacyHkdf) AND no
        // wrapped_account_key. authenticate() must re-derive the MasterKey
        // with HKDF and fall back to the MasterKey-derived AccountKey.
        let kdf = Kdf::LegacyHkdf;
        let mut store = AuthStore {
            salt: TEST_SALT.to_vec(),
            device_id: TEST_DEVICE.to_string(),
            kdf,
            personas: vec![
                AuthStore::build_entry(
                    b"Primary!Pass1234",
                    TEST_SALT,
                    TEST_DEVICE,
                    PRIMARY_PROBE,
                    &Kdf::LegacyHkdf,
                )
                .unwrap(),
                AuthStore::build_entry(
                    b"Duress!Pass5678",
                    TEST_SALT,
                    TEST_DEVICE,
                    DURESS_PROBE,
                    &Kdf::LegacyHkdf,
                )
                .unwrap(),
            ],
        };
        for entry in store.personas.iter_mut() {
            entry.wrapped_account_key = None;
        }
        let primary = store.authenticate(b"Primary!Pass1234").unwrap();

        // Equal to the value Phase 1 used to derive on the fly.
        let mk = MasterKey::from_passphrase(b"Primary!Pass1234", TEST_SALT).unwrap();
        let expected = AccountKey::derive(&mk).unwrap();
        assert_eq!(primary.account_key.as_bytes(), expected.as_bytes());
    }

    #[test]
    fn account_key_stable_across_device_ids() {
        // The v0.0.5 sync invariant at the AuthStore layer: two devices
        // that share a passphrase + salt produce the same AccountKey on
        // authenticate, even though their per-device DeviceKeys differ.
        // Verifies both the wrapped path (post-v0.0.5 stores) and the
        // legacy MasterKey-derived path (v0.0.4 stores).
        const SHARED_SALT: &[u8] = b"shared-salt-for-cross-device-test";
        let pw: &[u8] = b"SharedPass123!";
        let duress: &[u8] = b"DuressPass456@";

        // Two AuthStores with different device_ids but identical
        // passphrase + salt — what we'd see if the user runs onboarding
        // independently on two laptops sharing the same secrets (the
        // pre-pairing legacy path).
        let store_a =
            AuthStore::create(pw, duress, SHARED_SALT, "device-AAA").unwrap();
        let store_b =
            AuthStore::create(pw, duress, SHARED_SALT, "device-BBB").unwrap();

        let auth_a = store_a.authenticate(pw).unwrap();
        let auth_b = store_b.authenticate(pw).unwrap();

        // The DeviceKeys differ (per-device identity).
        assert_ne!(
            auth_a.device_key.as_bytes(),
            auth_b.device_key.as_bytes(),
            "DeviceKey must differ across device_ids"
        );

        // The AccountKeys match (user-scoped identity).
        assert_eq!(
            auth_a.account_key.as_bytes(),
            auth_b.account_key.as_bytes(),
            "AccountKey must be stable across device_id changes — this is the v0.0.5 sync invariant"
        );
    }

    #[test]
    fn new_store_uses_argon2id() {
        // CRYPTO-001: every freshly-created store must stretch with Argon2id.
        let store = AuthStore::create(
            b"Primary!Pass1234",
            b"Duress!Pass5678",
            TEST_SALT,
            TEST_DEVICE,
        )
        .unwrap();
        assert!(matches!(store.kdf, Kdf::Argon2id { .. }));
    }

    #[test]
    fn legacy_hkdf_store_still_unlocks() {
        // A pre-v0.0.7 store: personas built under HKDF, kdf=LegacyHkdf.
        // authenticate() must re-derive with HKDF and open it.
        let kdf = Kdf::LegacyHkdf;
        let primary =
            AuthStore::build_entry(b"Primary!Pass1234", TEST_SALT, TEST_DEVICE, PRIMARY_PROBE, &kdf)
                .unwrap();
        let duress =
            AuthStore::build_entry(b"Duress!Pass5678", TEST_SALT, TEST_DEVICE, DURESS_PROBE, &kdf)
                .unwrap();
        let store = AuthStore {
            salt: TEST_SALT.to_vec(),
            device_id: TEST_DEVICE.to_string(),
            kdf,
            personas: vec![primary, duress],
        };
        assert_eq!(
            store.authenticate(b"Primary!Pass1234").unwrap().persona,
            PersonaKind::Primary
        );
    }

    #[test]
    fn missing_kdf_field_defaults_to_legacy_and_opens() {
        // Old on-disk JSON has no "kdf" key. It must deserialize to
        // LegacyHkdf (serde default) and still authenticate.
        let kdf = Kdf::LegacyHkdf;
        let primary =
            AuthStore::build_entry(b"Primary!Pass1234", TEST_SALT, TEST_DEVICE, PRIMARY_PROBE, &kdf)
                .unwrap();
        let duress =
            AuthStore::build_entry(b"Duress!Pass5678", TEST_SALT, TEST_DEVICE, DURESS_PROBE, &kdf)
                .unwrap();
        let store = AuthStore {
            salt: TEST_SALT.to_vec(),
            device_id: TEST_DEVICE.to_string(),
            kdf,
            personas: vec![primary, duress],
        };
        let mut json: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&store).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("kdf"); // simulate a pre-field store
        let reparsed: AuthStore = serde_json::from_value(json).unwrap();
        assert_eq!(reparsed.kdf, Kdf::LegacyHkdf);
        assert_eq!(
            reparsed.authenticate(b"Primary!Pass1234").unwrap().persona,
            PersonaKind::Primary
        );
    }

    #[test]
    fn different_personas_have_different_keks() {
        let store = AuthStore::create(
            b"Primary!Pass1234",
            b"Duress!Pass5678",
            TEST_SALT,
            TEST_DEVICE,
        )
        .unwrap();

        let primary = store.authenticate(b"Primary!Pass1234").unwrap();
        let duress = store.authenticate(b"Duress!Pass5678").unwrap();
        assert_ne!(primary.kek.as_bytes(), duress.kek.as_bytes());
    }
}
