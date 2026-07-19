//! Proof-of-possession over a guardian-held shard (G3 heartbeat, and the
//! same primitive planned for mesh host audits, A5/F4).
//!
//! The owner sends a random nonce; the holder answers with a MAC keyed by
//! the shard bytes. A correct answer proves the shard is *still held* —
//! without ever putting shard bytes on the wire. The shard (a Shamir
//! share, 33 bytes for a 32-byte secret) is not itself a 32-byte MAC key,
//! so it is first compressed through HKDF-SHA256 under a fixed,
//! domain-separated info string; the MAC then runs under the derived key.
//!
//! Epoch binding: the info string carries the shard's rotation epoch, so
//! a pong recorded before a re-split can never satisfy a ping for the
//! new epoch.

use hkdf::Hkdf;
use sha2::Sha256;

use crate::error::{CryptoError, CryptoResult};
use crate::mac;

const POP_KDF_INFO: &str = "sovereign-guardian-pop:v1";
const POP_MAC_DOMAIN: &[u8] = b"sovereign-guardian-pop-mac:v1";

/// Derive the 32-byte proof-of-possession key for a shard at `epoch`.
fn pop_key(shard_bytes: &[u8], epoch: u32) -> CryptoResult<[u8; 32]> {
    if shard_bytes.is_empty() {
        return Err(CryptoError::RecoveryError("empty shard".into()));
    }
    let hk = Hkdf::<Sha256>::new(None, shard_bytes);
    let mut key = [0u8; 32];
    let info = format!("{POP_KDF_INFO}:{epoch}");
    hk.expand(info.as_bytes(), &mut key)
        .map_err(|e| CryptoError::DerivationFailed(e.to_string()))?;
    Ok(key)
}

/// Answer a heartbeat challenge: MAC(HKDF(shard, epoch), nonce), base64.
pub fn prove_possession(shard_bytes: &[u8], epoch: u32, nonce: &[u8]) -> CryptoResult<String> {
    let key = pop_key(shard_bytes, epoch)?;
    Ok(mac::keyed_mac(&key, POP_MAC_DOMAIN, nonce))
}

/// Owner-side verification of a heartbeat answer against the shard the
/// owner dealt to this guardian (the owner keeps a copy of what it dealt,
/// or re-derives it deterministically at re-split time).
pub fn verify_possession(
    shard_bytes: &[u8],
    epoch: u32,
    nonce: &[u8],
    proof_b64: &str,
) -> CryptoResult<bool> {
    let key = pop_key(shard_bytes, epoch)?;
    Ok(mac::verify_keyed_mac(&key, POP_MAC_DOMAIN, nonce, proof_b64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_roundtrip() {
        let shard = [7u8; 33];
        let nonce = [1u8; 32];
        let proof = prove_possession(&shard, 1, &nonce).unwrap();
        assert!(verify_possession(&shard, 1, &nonce, &proof).unwrap());
    }

    #[test]
    fn wrong_shard_fails() {
        let proof = prove_possession(&[7u8; 33], 1, &[1u8; 32]).unwrap();
        assert!(!verify_possession(&[8u8; 33], 1, &[1u8; 32], &proof).unwrap());
    }

    #[test]
    fn wrong_nonce_fails() {
        let shard = [7u8; 33];
        let proof = prove_possession(&shard, 1, &[1u8; 32]).unwrap();
        assert!(!verify_possession(&shard, 1, &[2u8; 32], &proof).unwrap());
    }

    #[test]
    fn epoch_is_bound() {
        // A pong recorded before a re-split must not satisfy the new epoch.
        let shard = [7u8; 33];
        let nonce = [1u8; 32];
        let proof = prove_possession(&shard, 1, &nonce).unwrap();
        assert!(!verify_possession(&shard, 2, &nonce, &proof).unwrap());
    }

    #[test]
    fn empty_shard_rejected() {
        assert!(prove_possession(&[], 1, &[1u8; 32]).is_err());
    }
}
