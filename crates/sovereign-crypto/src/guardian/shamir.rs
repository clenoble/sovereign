use blahaj::{Share, Sharks};

use crate::error::{CryptoError, CryptoResult};
use crate::master_key::MasterKey;

/// Default number of shares to generate.
pub const DEFAULT_TOTAL_SHARES: usize = 5;
/// Default threshold for reconstruction.
pub const DEFAULT_THRESHOLD: u8 = 3;

/// Split a master key into Shamir shares.
///
/// Returns `total` shares; any `threshold` of them can reconstruct the key.
pub fn split_master_key(
    key: &MasterKey,
    threshold: u8,
    total: usize,
) -> CryptoResult<Vec<Share>> {
    if total < threshold as usize {
        return Err(CryptoError::RecoveryError(format!(
            "total ({}) must be >= threshold ({})",
            total, threshold
        )));
    }
    if threshold < 2 {
        return Err(CryptoError::RecoveryError(
            "threshold must be >= 2".into(),
        ));
    }

    let sharks = Sharks(threshold);
    let shares: Vec<Share> = sharks.dealer(key.as_bytes()).take(total).collect();
    Ok(shares)
}

/// Reconstruct a master key from Shamir shares.
///
/// Requires at least `threshold` valid shares.
pub fn reconstruct(shares: &[Share], threshold: u8) -> CryptoResult<MasterKey> {
    if shares.len() < threshold as usize {
        return Err(CryptoError::InsufficientShards {
            threshold,
            got: shares.len(),
        });
    }

    let sharks = Sharks(threshold);
    let secret = sharks
        .recover(shares)
        .map_err(|e| CryptoError::RecoveryError(format!("reconstruction failed: {e}")))?;

    if secret.len() != 32 {
        return Err(CryptoError::InvalidKeyLength {
            expected: 32,
            got: secret.len(),
        });
    }

    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&secret);
    Ok(MasterKey::from_bytes(bytes))
}

/// Serialize a Share to bytes for storage/transport.
pub fn share_to_bytes(share: &Share) -> Vec<u8> {
    Vec::from(share)
}

/// Deserialize a Share from bytes.
pub fn share_from_bytes(bytes: &[u8]) -> CryptoResult<Share> {
    Share::try_from(bytes)
        .map_err(|e| CryptoError::RecoveryError(format!("invalid share bytes: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_reconstruct_3_of_5() {
        let mk = MasterKey::generate();
        let shares = split_master_key(&mk, 3, 5).unwrap();
        assert_eq!(shares.len(), 5);

        // Any 3 shares should work
        let subset: Vec<Share> = shares[0..3].to_vec();
        let recovered = reconstruct(&subset, 3).unwrap();
        assert_eq!(mk.as_bytes(), recovered.as_bytes());
    }

    #[test]
    fn reconstruct_with_different_3_of_5() {
        let mk = MasterKey::generate();
        let shares = split_master_key(&mk, 3, 5).unwrap();

        // Shares 2, 3, 4 should also work
        let subset: Vec<Share> = shares[2..5].to_vec();
        let recovered = reconstruct(&subset, 3).unwrap();
        assert_eq!(mk.as_bytes(), recovered.as_bytes());
    }

    #[test]
    fn reconstruct_with_2_of_5_fails() {
        let mk = MasterKey::generate();
        let shares = split_master_key(&mk, 3, 5).unwrap();
        let subset: Vec<Share> = shares[0..2].to_vec();
        assert!(reconstruct(&subset, 3).is_err());
    }

    #[test]
    fn share_serde_roundtrip() {
        let mk = MasterKey::generate();
        let shares = split_master_key(&mk, 3, 5).unwrap();

        let bytes = share_to_bytes(&shares[0]);
        let recovered_share = share_from_bytes(&bytes).unwrap();

        // Use the recovered share in reconstruction
        let mut subset = vec![recovered_share];
        subset.extend_from_slice(&shares[1..3]);
        let recovered = reconstruct(&subset, 3).unwrap();
        assert_eq!(mk.as_bytes(), recovered.as_bytes());
    }

    #[test]
    fn passphrase_derived_key_split_recover() {
        let mk = MasterKey::from_passphrase(b"my strong passphrase", b"unique-salt").unwrap();
        let shares = split_master_key(&mk, 3, 5).unwrap();
        let subset: Vec<Share> = shares[1..4].to_vec();
        let recovered = reconstruct(&subset, 3).unwrap();
        assert_eq!(mk.as_bytes(), recovered.as_bytes());
    }

    #[test]
    fn threshold_too_low_rejected() {
        let mk = MasterKey::generate();
        assert!(split_master_key(&mk, 1, 5).is_err());
    }

    #[test]
    fn total_less_than_threshold_rejected() {
        let mk = MasterKey::generate();
        assert!(split_master_key(&mk, 4, 3).is_err());
    }
}
