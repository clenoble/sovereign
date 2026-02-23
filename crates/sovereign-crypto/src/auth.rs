use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::aead::{self, NONCE_SIZE};
use crate::device_key::DeviceKey;
use crate::error::{CryptoError, CryptoResult};
use crate::kek::{Kek, WrappedKek};
use crate::master_key::MasterKey;

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
    /// Random opaque label — NOT "primary" / "duress".
    pub label: [u8; 16],
}

/// Successful authentication result.
pub struct AuthSuccess {
    pub persona: PersonaKind,
    pub device_key: DeviceKey,
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
        let primary_entry =
            Self::build_entry(primary_passphrase, salt, device_id, PRIMARY_PROBE)?;
        let duress_entry =
            Self::build_entry(duress_passphrase, salt, device_id, DURESS_PROBE)?;

        // Randomize order so file inspection can't correlate position with kind.
        let mut personas = vec![primary_entry, duress_entry];
        if rand::rng().random_bool(0.5) {
            personas.swap(0, 1);
        }

        Ok(Self {
            salt: salt.to_vec(),
            device_id: device_id.to_string(),
            personas,
        })
    }

    /// Try to authenticate with a passphrase.
    /// Derives keys, tries each persona probe. Returns the matching persona,
    /// DeviceKey, and unwrapped KEK on success.
    pub fn authenticate(&self, passphrase: &[u8]) -> CryptoResult<AuthSuccess> {
        let master = MasterKey::from_passphrase(passphrase, &self.salt)?;
        let device_key = DeviceKey::derive(&master, &self.device_id)?;

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
                return Ok(AuthSuccess {
                    persona,
                    device_key,
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
        std::fs::write(path, json).map_err(|e| CryptoError::KeyDbIo(e.to_string()))?;
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
    ) -> CryptoResult<PersonaEntry> {
        let master = MasterKey::from_passphrase(passphrase, salt)?;
        let device_key = DeviceKey::derive(&master, device_id)?;
        let kek = Kek::generate();
        let wrapped_kek = kek.wrap(&device_key)?;
        let (probe_ciphertext, probe_nonce) =
            aead::encrypt(probe_plaintext, device_key.as_bytes())?;
        let mut label = [0u8; 16];
        rand::rng().fill_bytes(&mut label);

        Ok(PersonaEntry {
            probe_ciphertext,
            probe_nonce,
            wrapped_kek,
            label,
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
