//! Pairing offer + handshake material for the P3.1 interactive pairing
//! flow (CRYPTO-003 part 2).
//!
//! The QR no longer carries the AccountKey (or any secret at all). It
//! carries a short-lived plaintext **offer** — the existing device's
//! PeerId, optional dial addresses, a random offer id, and an expiry.
//! Photographing the QR is harmless: everything in it is discoverable on
//! the LAN via mDNS anyway, and there is nothing to brute-force offline.
//!
//! The 50-bit pairing code (displayed by the existing device, typed by
//! the user — see `sovereign_crypto::pair_payload`) never appears in any
//! offline artifact. Both sides stretch it with Argon2id, salted by the
//! offer id, into the **handshake key** `K`. The new device then proves
//! knowledge of the code online over a single-use, attempt-capped,
//! TTL-bound challenge/response, and only then does the existing device
//! send the AccountKey + salt sealed under `K` (the transport is already
//! Noise-encrypted to the offer's PeerId; sealing under `K` binds
//! delivery to code knowledge as defense in depth).
//!
//! Handshake (request/response pairs on `/sovereign/sync/1`):
//!   1. B→A `PairHello { offer_id, device_name }`
//!      A→B `PairChallenge { nonce }`
//!   2. B→A `PairProof { offer_id, proof = MAC(K, nonce ‖ dialer peer) }`
//!      A→B `PairGranted { sealed PairSecrets under K }`
//!   3. B→A `PairComplete { final_peer_id, mac = MAC(K, nonce ‖ final) }`
//!      A→B `PairDone`
//!
//! Step 3 exists because B's *final* libp2p identity is derived from the
//! DeviceKey, which needs the MasterKey salt B only learns in step 2 —
//! the handshake itself runs over an ephemeral identity. On `PairDone`
//! the existing device registers the final peer id as paired (allow-list
//! + per-pair sealing key), closing the loop that v0.0.5 left open
//! (the source device never used to learn the new device's identity).

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{P2pError, P2pResult};

/// Schema version of the plaintext offer. The PIN-encrypted v1 payload
/// (`EncryptedPairPayload`) decodes as a different JSON shape, so the two
/// formats can't be confused.
const OFFER_VERSION: u8 = 2;

/// Offer lifetime. Longer than the old QR's 60s because the new flow
/// includes dialing + the typed code + (on slow devices) two Argon2id
/// stretches before the first message arrives.
pub const OFFER_TTL_SECONDS: i64 = 120;

/// How many wrong proofs are tolerated before the offer self-destructs.
/// With a 50-bit code, 3 online guesses are negligible (~2⁻⁴⁸).
pub const MAX_PROOF_ATTEMPTS: u8 = 3;

/// Plaintext pairing offer carried by the QR. No secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingOffer {
    pub schema_version: u8,
    /// Random 16-byte id, base64url. Doubles as the Argon2id salt for
    /// the handshake-key derivation, so a code guess against one offer
    /// is useless against another.
    pub offer_id: String,
    /// The existing device's libp2p PeerId. The new device dials it with
    /// the `/p2p/<peer>` suffix, so the QUIC/Noise handshake itself
    /// authenticates the responder against the scanned offer.
    pub source_peer_id: String,
    /// Optional dial hints (multiaddrs). When empty the client falls
    /// back to mDNS discovery of `source_peer_id` on the LAN.
    #[serde(default)]
    pub addrs: Vec<String>,
    pub source_device_name: String,
    /// Unix milliseconds.
    pub issued_at: i64,
    pub expires_at: i64,
}

impl PairingOffer {
    pub fn new(
        source_peer_id: String,
        source_device_name: String,
        addrs: Vec<String>,
        ttl_seconds: i64,
    ) -> Self {
        let mut id_bytes = [0u8; 16];
        rand::rng().fill_bytes(&mut id_bytes);
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            schema_version: OFFER_VERSION,
            offer_id: B64URL.encode(id_bytes),
            source_peer_id,
            source_device_name,
            addrs,
            issued_at: now,
            expires_at: now + ttl_seconds * 1000,
        }
    }

    /// Encode for QR display (base64url of JSON).
    pub fn encode(&self) -> P2pResult<String> {
        let json = serde_json::to_vec(self)
            .map_err(|e| P2pError::PairingError(format!("offer encode: {e}")))?;
        Ok(B64URL.encode(&json))
    }

    /// Decode a scanned offer. Rejects unknown versions and expired offers.
    pub fn decode(offer_b64: &str) -> P2pResult<Self> {
        let bytes = B64URL
            .decode(offer_b64)
            .map_err(|e| P2pError::PairingError(format!("offer base64: {e}")))?;
        let offer: Self = serde_json::from_slice(&bytes)
            .map_err(|e| P2pError::PairingError(format!("offer decode: {e}")))?;
        if offer.schema_version != OFFER_VERSION {
            return Err(P2pError::PairingError(format!(
                "unsupported offer version {}",
                offer.schema_version
            )));
        }
        if offer.expired() {
            return Err(P2pError::PairingError("pairing offer expired".into()));
        }
        Ok(offer)
    }

    pub fn expired(&self) -> bool {
        chrono::Utc::now().timestamp_millis() > self.expires_at
    }

    /// The raw offer-id bytes, used as the Argon2id salt for
    /// [`derive_handshake_key`].
    pub fn kdf_salt(&self) -> P2pResult<Vec<u8>> {
        B64URL
            .decode(&self.offer_id)
            .map_err(|e| P2pError::PairingError(format!("offer id base64: {e}")))
    }
}

/// Stretch the user-typed pairing code into the handshake key `K` for
/// this offer. Argon2id (t=2, m=64MiB) via the same KDF the old QR
/// encryption used; both sides call this with the same inputs.
pub fn derive_handshake_key(code: &str, offer: &PairingOffer) -> P2pResult<[u8; 32]> {
    let salt = offer.kdf_salt()?;
    sovereign_crypto::pair_payload::derive_code_key(code, &salt)
        .map_err(|e| P2pError::PairingError(format!("handshake key: {e}")))
}

const PROOF_CONTEXT: &str = "sovereign-pair-proof:v1";
const CONFIRM_CONTEXT: &str = "sovereign-pair-confirm:v1";

/// MAC proving code knowledge, bound to the challenge nonce, the offer,
/// and the dialing (ephemeral) peer id — a captured proof can't be
/// replayed from another connection or against another offer.
pub fn proof_mac(key: &[u8; 32], offer_id: &str, nonce: &[u8], dialer_peer_id: &str) -> Vec<u8> {
    sovereign_crypto::pair_payload::handshake_mac(
        key,
        PROOF_CONTEXT,
        &[offer_id.as_bytes(), nonce, dialer_peer_id.as_bytes()],
    )
}

pub fn verify_proof_mac(
    key: &[u8; 32],
    offer_id: &str,
    nonce: &[u8],
    dialer_peer_id: &str,
    tag: &[u8],
) -> bool {
    sovereign_crypto::pair_payload::verify_handshake_mac(
        key,
        PROOF_CONTEXT,
        &[offer_id.as_bytes(), nonce, dialer_peer_id.as_bytes()],
        tag,
    )
}

/// MAC binding the new device's FINAL peer id (derived after it received
/// the salt) to this handshake session.
pub fn confirm_mac(key: &[u8; 32], offer_id: &str, nonce: &[u8], final_peer_id: &str) -> Vec<u8> {
    sovereign_crypto::pair_payload::handshake_mac(
        key,
        CONFIRM_CONTEXT,
        &[offer_id.as_bytes(), nonce, final_peer_id.as_bytes()],
    )
}

pub fn verify_confirm_mac(
    key: &[u8; 32],
    offer_id: &str,
    nonce: &[u8],
    final_peer_id: &str,
    tag: &[u8],
) -> bool {
    sovereign_crypto::pair_payload::verify_handshake_mac(
        key,
        CONFIRM_CONTEXT,
        &[offer_id.as_bytes(), nonce, final_peer_id.as_bytes()],
        tag,
    )
}

/// The secrets the existing device releases after a successful proof:
/// the MasterKey salt + the AccountKey (what the old QR used to carry),
/// sealed under the handshake key for transport.
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct PairSecrets {
    pub salt: Vec<u8>,
    pub account_key_bytes: [u8; 32],
    pub source_device_name: String,
}

impl std::fmt::Debug for PairSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairSecrets")
            .field("account_key_bytes", &"[REDACTED]")
            .field("source_device_name", &self.source_device_name)
            .finish_non_exhaustive()
    }
}

impl PairSecrets {
    /// AEAD-seal under the handshake key. Returns (ciphertext_b64, nonce_b64).
    pub fn seal(&self, key: &[u8; 32]) -> P2pResult<(String, String)> {
        let json = serde_json::to_vec(self)
            .map_err(|e| P2pError::PairingError(format!("secrets encode: {e}")))?;
        let (ct, nonce) = sovereign_crypto::aead::encrypt(&json, key)
            .map_err(|e| P2pError::PairingError(format!("secrets seal: {e}")))?;
        Ok((
            base64::engine::general_purpose::STANDARD.encode(&ct),
            base64::engine::general_purpose::STANDARD.encode(nonce),
        ))
    }

    pub fn unseal(ciphertext_b64: &str, nonce_b64: &str, key: &[u8; 32]) -> P2pResult<Self> {
        let ct = base64::engine::general_purpose::STANDARD
            .decode(ciphertext_b64)
            .map_err(|e| P2pError::PairingError(format!("secrets b64: {e}")))?;
        let nonce_bytes = base64::engine::general_purpose::STANDARD
            .decode(nonce_b64)
            .map_err(|e| P2pError::PairingError(format!("secrets b64: {e}")))?;
        if nonce_bytes.len() != 24 {
            return Err(P2pError::PairingError("secrets nonce length".into()));
        }
        let mut nonce = [0u8; 24];
        nonce.copy_from_slice(&nonce_bytes);
        let plaintext = sovereign_crypto::aead::decrypt(&ct, &nonce, key)
            .map_err(|_| P2pError::PairingError("secrets unseal failed".into()))?;
        serde_json::from_slice(&plaintext)
            .map_err(|e| P2pError::PairingError(format!("secrets decode: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer() -> PairingOffer {
        PairingOffer::new(
            "12D3KooWSourcePeer".into(),
            "Alice's laptop".into(),
            vec!["/ip4/192.168.1.5/udp/4001/quic-v1".into()],
            OFFER_TTL_SECONDS,
        )
    }

    #[test]
    fn offer_encode_decode_roundtrip() {
        let o = offer();
        let b64 = o.encode().unwrap();
        let back = PairingOffer::decode(&b64).unwrap();
        assert_eq!(back.offer_id, o.offer_id);
        assert_eq!(back.source_peer_id, "12D3KooWSourcePeer");
        assert_eq!(back.addrs.len(), 1);
    }

    #[test]
    fn expired_offer_rejected_on_decode() {
        let mut o = offer();
        o.expires_at = chrono::Utc::now().timestamp_millis() - 1_000;
        let b64 = o.encode().unwrap();
        assert!(PairingOffer::decode(&b64).is_err());
    }

    #[test]
    fn offer_carries_no_secret_material() {
        // The whole point of P3.1: the encoded offer must not contain
        // anything beyond public, LAN-discoverable metadata.
        let o = offer();
        let json = String::from_utf8(
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(o.encode().unwrap())
                .unwrap(),
        )
        .unwrap();
        assert!(!json.contains("account_key"), "no key material in the QR");
        assert!(!json.contains("salt\""), "no MasterKey salt in the QR");
        assert!(!json.contains("code"), "no pairing code in the QR");
    }

    #[test]
    fn handshake_key_is_offer_scoped() {
        let o1 = offer();
        let o2 = offer();
        let k1 = derive_handshake_key("ABCDE-FGHJK", &o1).unwrap();
        let k1b = derive_handshake_key("abcdefghjk", &o1).unwrap();
        let k2 = derive_handshake_key("ABCDE-FGHJK", &o2).unwrap();
        assert_eq!(k1, k1b, "code normalization applies");
        assert_ne!(k1, k2, "different offers must derive different keys");
    }

    #[test]
    fn proof_and_confirm_macs_are_bound() {
        let o = offer();
        let k = derive_handshake_key("ABCDE-FGHJK", &o).unwrap();
        let nonce = [9u8; 32];

        let proof = proof_mac(&k, &o.offer_id, &nonce, "12D3KooWDialer");
        assert!(verify_proof_mac(&k, &o.offer_id, &nonce, "12D3KooWDialer", &proof));
        assert!(!verify_proof_mac(&k, &o.offer_id, &nonce, "12D3KooWOther", &proof));
        assert!(!verify_proof_mac(&k, "other-offer", &nonce, "12D3KooWDialer", &proof));
        assert!(!verify_proof_mac(&k, &o.offer_id, &[8u8; 32], "12D3KooWDialer", &proof));
        // Proof and confirm are domain-separated.
        assert!(!verify_confirm_mac(&k, &o.offer_id, &nonce, "12D3KooWDialer", &proof));

        let confirm = confirm_mac(&k, &o.offer_id, &nonce, "12D3KooWFinal");
        assert!(verify_confirm_mac(&k, &o.offer_id, &nonce, "12D3KooWFinal", &confirm));
        assert!(!verify_confirm_mac(&k, &o.offer_id, &nonce, "12D3KooWOther", &confirm));
    }

    #[test]
    fn secrets_seal_unseal_roundtrip() {
        let o = offer();
        let k = derive_handshake_key("ABCDE-FGHJK", &o).unwrap();
        let secrets = PairSecrets {
            salt: b"master-salt".to_vec(),
            account_key_bytes: [0xAB; 32],
            source_device_name: "Alice's laptop".into(),
        };
        let (ct, nonce) = secrets.seal(&k).unwrap();
        let back = PairSecrets::unseal(&ct, &nonce, &k).unwrap();
        assert_eq!(back.salt, b"master-salt");
        assert_eq!(back.account_key_bytes, [0xAB; 32]);

        // Wrong key (wrong code) cannot unseal.
        let wrong = derive_handshake_key("WRONG-CODEE", &o).unwrap();
        assert!(PairSecrets::unseal(&ct, &nonce, &wrong).is_err());
    }
}
