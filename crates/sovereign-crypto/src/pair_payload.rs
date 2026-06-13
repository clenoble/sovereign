//! Out-of-band pairing payload for v0.0.5 mobile + desktop sync.
//!
//! When a user pairs a new device with an existing one, the existing
//! device displays a QR code (or 6-word fallback) plus a 10-symbol
//! pairing code (50 bits of entropy; see CRYPTO-003). The QR carries
//! the user's `salt` + `AccountKey` + the existing device's
//! `PeerId`/name + a 60-second expiry, AEAD-encrypted under a key
//! derived from the pairing code via Argon2id. The new device scans
//! the QR, the user types the code, and the new device unwraps the
//! payload to import the AccountKey.
//!
//! Threat model:
//! - QR + pairing code travel via different channels (visual + verbal).
//!   An attacker who only sees the QR can't decrypt without the code.
//! - Argon2id (t=2, m=64MiB, p=1) over a 50-bit code makes offline
//!   brute-force of a captured QR computationally infeasible
//!   (~9 million years on average at ~250–500 ms/guess).
//! - 60-second expiry limits the attack window; the existing device
//!   regenerates the QR + code every time the pairing screen is opened.
//! - The code is single-use: the existing device clears the
//!   `PendingPairing` after a successful consume.
//!
//! For v0.0.5 we don't add an interactive ECDH handshake on the wire —
//! mDNS-only deployment + QUIC's Noise + TLS handles connection privacy
//! between known peers. v0.0.6 will revisit when WAN/relay lands.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::aead::{self, KEY_SIZE, NONCE_SIZE};
use crate::error::{CryptoError, CryptoResult};

/// Schema-version byte at the start of the unencrypted CBOR-of-fields
/// blob. Bumped on incompatible changes.
const PAIR_PAYLOAD_VERSION: u8 = 1;

/// Default expiry for a pairing QR / code. The user has this window to
/// scan + enter the PIN before the existing device must regenerate.
pub const PAIR_TTL_SECONDS: i64 = 60;

/// Argon2id salt size for PIN derivation (separate from the user's
/// MasterKey salt that travels in `PairPayload.salt`).
const PIN_KDF_SALT_SIZE: usize = 16;

/// Plaintext pairing payload. Contains everything a new device needs
/// to derive the AccountKey locally:
///   - `salt`: the user's MasterKey salt (so the new device's local
///     passphrase derives a compatible MasterKey path if the user
///     re-uses the same passphrase, though this is optional)
///   - `account_key_bytes`: the imported AccountKey itself (32 bytes)
///   - `source_peer_id`: existing device's libp2p PeerId, recorded as
///     the first paired device on the new device
///   - `source_device_name`: human-readable name shown in Settings
///   - `issued_at`/`expires_at`: unix milliseconds
#[derive(Clone, Serialize, Deserialize, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct PairPayload {
    pub schema_version: u8,
    pub salt: Vec<u8>,
    pub account_key_bytes: [u8; KEY_SIZE],
    pub source_peer_id: String,
    pub source_device_name: String,
    pub issued_at: i64,
    pub expires_at: i64,
}

// Manual Debug: the derived impl would print the raw AccountKey bytes into
// any log/format site (every other key type in this crate redacts).
impl std::fmt::Debug for PairPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairPayload")
            .field("schema_version", &self.schema_version)
            .field("account_key_bytes", &"[REDACTED]")
            .field("source_peer_id", &self.source_peer_id)
            .field("source_device_name", &self.source_device_name)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl PairPayload {
    pub fn new(
        salt: Vec<u8>,
        account_key_bytes: [u8; KEY_SIZE],
        source_peer_id: String,
        source_device_name: String,
        ttl_seconds: i64,
    ) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            schema_version: PAIR_PAYLOAD_VERSION,
            salt,
            account_key_bytes,
            source_peer_id,
            source_device_name,
            issued_at: now,
            expires_at: now + ttl_seconds * 1000,
        }
    }

    /// Encrypt under a key derived from `pin` via Argon2id. Returns the
    /// envelope ready to be base64-encoded into a QR or 6-word code.
    pub fn encrypt(&self, pin: &str) -> CryptoResult<EncryptedPairPayload> {
        let plaintext = serde_json::to_vec(self)
            .map_err(|e| CryptoError::PairPayload(format!("serialize: {e}")))?;
        let mut salt_for_pin = [0u8; PIN_KDF_SALT_SIZE];
        rand::rng().fill_bytes(&mut salt_for_pin);
        let key = derive_pin_key(pin, &salt_for_pin)?;
        let (ciphertext, nonce) = aead::encrypt(&plaintext, &key)
            .map_err(|e| CryptoError::PairPayload(format!("aead: {e}")))?;
        Ok(EncryptedPairPayload {
            schema_version: PAIR_PAYLOAD_VERSION,
            ciphertext,
            nonce,
            salt_for_pin,
        })
    }
}

/// PIN-encrypted envelope. Wire format: base64url-no-pad of
/// `serde_json::to_vec(EncryptedPairPayload)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPairPayload {
    pub schema_version: u8,
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; NONCE_SIZE],
    pub salt_for_pin: [u8; PIN_KDF_SALT_SIZE],
}

impl EncryptedPairPayload {
    /// Encode for transport over a QR code or 6-word channel.
    pub fn encode(&self) -> CryptoResult<String> {
        let bytes = serde_json::to_vec(self)
            .map_err(|e| CryptoError::PairPayload(format!("encode: {e}")))?;
        Ok(B64.encode(&bytes))
    }

    /// Decode from a base64url QR/code string.
    pub fn decode(payload_b64: &str) -> CryptoResult<Self> {
        let bytes = B64
            .decode(payload_b64)
            .map_err(|e| CryptoError::PairPayload(format!("base64: {e}")))?;
        let parsed: Self = serde_json::from_slice(&bytes)
            .map_err(|e| CryptoError::PairPayload(format!("decode: {e}")))?;
        if parsed.schema_version != PAIR_PAYLOAD_VERSION {
            return Err(CryptoError::PairPayload(format!(
                "unsupported schema version: {}",
                parsed.schema_version
            )));
        }
        Ok(parsed)
    }

    /// Decrypt under the PIN. Verifies the inner payload's `expires_at`
    /// against the current time and rejects expired payloads (so a
    /// stale QR can't be redeemed long after it was generated).
    pub fn decrypt(&self, pin: &str) -> CryptoResult<PairPayload> {
        let key = derive_pin_key(pin, &self.salt_for_pin)?;
        let plaintext = aead::decrypt(&self.ciphertext, &self.nonce, &key)
            .map_err(|_| CryptoError::PairPayload("PIN decryption failed".into()))?;
        let payload: PairPayload = serde_json::from_slice(&plaintext)
            .map_err(|e| CryptoError::PairPayload(format!("inner decode: {e}")))?;
        let now = chrono::Utc::now().timestamp_millis();
        if now > payload.expires_at {
            return Err(CryptoError::PairPayload("pair payload expired".into()));
        }
        Ok(payload)
    }
}

/// Pairing-code alphabet: Crockford-style base32 minus the ambiguous
/// letters I, L, O, U. 32 symbols → 5 bits each.
const CODE_ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Number of random symbols in a pairing code. 10 × 5 bits = 50 bits of
/// entropy — at the Argon2id cost below (~250–500 ms/guess) an offline
/// brute-force of a captured QR is ~9 million years on average, vs the
/// old 6-digit PIN (20 bits ≈ crackable in days). See CRYPTO-003.
const CODE_LEN: usize = 10;

/// Normalize a user-typed pairing code before key derivation: uppercase
/// and drop separators/whitespace, so "abcde-fghjk" and "ABCDEFGHJK" are
/// equivalent. Derivation always runs on the normalized form on BOTH the
/// generating and consuming sides.
fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// Argon2id with t=2, m=64 MiB, p=1. Stretches a pairing code to a
/// 256-bit key. Input is normalized first so the code is case- and
/// separator-insensitive. Public since P3.1: the interactive pairing
/// handshake derives its session key from the code + the offer id with
/// the same KDF (`derive_code_key` is the public name).
pub fn derive_code_key(code: &str, salt: &[u8]) -> CryptoResult<[u8; KEY_SIZE]> {
    derive_pin_key(code, salt)
}

/// See [`derive_code_key`].
fn derive_pin_key(code: &str, salt: &[u8]) -> CryptoResult<[u8; KEY_SIZE]> {
    use argon2::{Algorithm, Argon2, Params, Version};

    let normalized = normalize_code(code);
    let params = Params::new(64 * 1024, 2, 1, Some(KEY_SIZE))
        .map_err(|e| CryptoError::PairPayload(format!("argon2 params: {e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; KEY_SIZE];
    argon
        .hash_password_into(normalized.as_bytes(), salt, &mut out)
        .map_err(|e| CryptoError::PairPayload(format!("argon2: {e}")))?;
    Ok(out)
}

/// HMAC-SHA256 tag under a handshake key, domain-separated by `context`
/// and computed over the length-prefixed `parts` (no delimiter
/// ambiguity). Used by the P3.1 pairing handshake for the proof and
/// confirm MACs.
pub fn handshake_mac(key: &[u8; KEY_SIZE], context: &str, parts: &[&[u8]]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    let mut mac = <Hmac<sha2::Sha256> as Mac>::new_from_slice(key)
        .expect("HMAC accepts any key length");
    mac.update(context.as_bytes());
    for p in parts {
        mac.update(&(p.len() as u32).to_le_bytes());
        mac.update(p);
    }
    mac.finalize().into_bytes().to_vec()
}

/// Constant-time verification of a [`handshake_mac`] tag.
pub fn verify_handshake_mac(
    key: &[u8; KEY_SIZE],
    context: &str,
    parts: &[&[u8]],
    tag: &[u8],
) -> bool {
    use hmac::{Hmac, Mac};
    let mut mac = <Hmac<sha2::Sha256> as Mac>::new_from_slice(key)
        .expect("HMAC accepts any key length");
    mac.update(context.as_bytes());
    for p in parts {
        mac.update(&(p.len() as u32).to_le_bytes());
        mac.update(p);
    }
    mac.verify_slice(tag).is_ok()
}

/// Generate a fresh high-entropy pairing code (50 bits). Returned grouped
/// as `XXXXX-XXXXX` for readability; the dash is cosmetic
/// (`normalize_code` strips it). Single-use; the existing device discards
/// it after a successful consume or after `PAIR_TTL_SECONDS`.
pub fn generate_pairing_code() -> String {
    let mut rng = rand::rng();
    let chars: Vec<char> = (0..CODE_LEN)
        .map(|_| {
            let idx = rng.random_range(0..CODE_ALPHABET.len());
            CODE_ALPHABET[idx] as char
        })
        .collect();
    let mid = CODE_LEN / 2;
    format!(
        "{}-{}",
        chars[..mid].iter().collect::<String>(),
        chars[mid..].iter().collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_key::AccountKey;
    use crate::master_key::MasterKey;

    fn sample_payload() -> PairPayload {
        let mk = MasterKey::from_passphrase(b"test", b"shared-salt").unwrap();
        let ak = AccountKey::derive(&mk).unwrap();
        PairPayload::new(
            b"shared-salt".to_vec(),
            *ak.as_bytes(),
            "12D3KooW...".into(),
            "alice's laptop".into(),
            PAIR_TTL_SECONDS,
        )
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let payload = sample_payload();
        let encrypted = payload.encrypt("123456").unwrap();
        let decoded_b64 = encrypted.encode().unwrap();
        let parsed = EncryptedPairPayload::decode(&decoded_b64).unwrap();
        let decrypted = parsed.decrypt("123456").unwrap();
        assert_eq!(decrypted.account_key_bytes, payload.account_key_bytes);
        assert_eq!(decrypted.source_device_name, "alice's laptop");
    }

    #[test]
    fn wrong_pin_fails() {
        let payload = sample_payload();
        let encrypted = payload.encrypt("123456").unwrap();
        assert!(encrypted.decrypt("000000").is_err());
    }

    #[test]
    fn expired_payload_rejected() {
        let mut payload = sample_payload();
        payload.expires_at = chrono::Utc::now().timestamp_millis() - 60_000;
        let encrypted = payload.encrypt("123456").unwrap();
        let err = encrypted.decrypt("123456").unwrap_err();
        match err {
            CryptoError::PairPayload(msg) => assert!(msg.contains("expired")),
            _ => panic!("expected PairPayload(expired), got {err:?}"),
        }
    }

    #[test]
    fn generate_pairing_code_is_high_entropy() {
        for _ in 0..100 {
            let code = generate_pairing_code();
            // "XXXXX-XXXXX": 10 symbols after normalization (dash stripped).
            let norm = normalize_code(&code);
            assert_eq!(norm.len(), 10, "10 symbols => 50 bits of entropy");
            assert!(norm.chars().all(|c| c.is_ascii_alphanumeric()));
            // Ambiguous letters excluded from the alphabet.
            assert!(!norm.contains(['I', 'L', 'O', 'U']));
        }
        // Two fresh codes overwhelmingly differ.
        assert_ne!(generate_pairing_code(), generate_pairing_code());
    }

    #[test]
    fn pairing_code_is_case_and_separator_insensitive() {
        // CRYPTO-003: derivation normalizes, so the user can type the code
        // lowercase / without the dash and still decrypt.
        let payload = sample_payload();
        let encrypted = payload.encrypt("ABCDE-FGHJK").unwrap();
        let decrypted = encrypted.decrypt("abcdefghjk").unwrap();
        assert_eq!(decrypted.account_key_bytes, payload.account_key_bytes);
    }

    #[test]
    fn schema_version_mismatch_rejected() {
        let payload = sample_payload();
        let mut encrypted = payload.encrypt("123456").unwrap();
        encrypted.schema_version = 99;
        let b64 = encrypted.encode().unwrap();
        assert!(EncryptedPairPayload::decode(&b64).is_err());
    }

    #[test]
    fn handshake_mac_roundtrip_and_separation() {
        let key = [3u8; KEY_SIZE];
        let tag = handshake_mac(&key, "ctx:v1", &[b"nonce", b"peer"]);
        assert!(verify_handshake_mac(&key, "ctx:v1", &[b"nonce", b"peer"], &tag));

        // Wrong key, wrong context, reordered/merged parts all fail.
        assert!(!verify_handshake_mac(&[4u8; KEY_SIZE], "ctx:v1", &[b"nonce", b"peer"], &tag));
        assert!(!verify_handshake_mac(&key, "ctx:v2", &[b"nonce", b"peer"], &tag));
        assert!(!verify_handshake_mac(&key, "ctx:v1", &[b"peer", b"nonce"], &tag));
        assert!(
            !verify_handshake_mac(&key, "ctx:v1", &[b"noncepeer"], &tag),
            "length prefixing must prevent part-boundary ambiguity"
        );
        let mut bad = tag.clone();
        bad[0] ^= 1;
        assert!(!verify_handshake_mac(&key, "ctx:v1", &[b"nonce", b"peer"], &bad));
    }

    #[test]
    fn derive_code_key_is_normalized_and_deterministic() {
        let k1 = derive_code_key("ABCDE-FGHJK", b"offer-id-salt-16").unwrap();
        let k2 = derive_code_key("abcde fghjk", b"offer-id-salt-16").unwrap();
        assert_eq!(k1, k2, "normalization must apply");
        let k3 = derive_code_key("ABCDE-FGHJK", b"other-salt-bytes").unwrap();
        assert_ne!(k1, k3, "salt must matter");
    }

    #[test]
    fn payload_includes_source_peer_id() {
        let payload = sample_payload();
        let encrypted = payload.encrypt("123456").unwrap();
        let parsed = EncryptedPairPayload::decode(&encrypted.encode().unwrap()).unwrap();
        let decrypted = parsed.decrypt("123456").unwrap();
        assert!(!decrypted.source_peer_id.is_empty());
    }
}
