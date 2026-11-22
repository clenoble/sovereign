use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    XChaCha20Poly1305,
};

use crate::error::{CryptoError, CryptoResult};

pub const KEY_SIZE: usize = 32;
pub const NONCE_SIZE: usize = 24;

/// Encrypt plaintext with XChaCha20-Poly1305.
/// Returns (ciphertext, nonce) pair.
pub fn encrypt(plaintext: &[u8], key: &[u8; KEY_SIZE]) -> CryptoResult<(Vec<u8>, [u8; NONCE_SIZE])> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| CryptoError::EncryptionFailed)?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    nonce_bytes.copy_from_slice(&nonce);
    Ok((ciphertext, nonce_bytes))
}

/// Decrypt ciphertext with XChaCha20-Poly1305.
pub fn decrypt(
    ciphertext: &[u8],
    nonce: &[u8; NONCE_SIZE],
    key: &[u8; KEY_SIZE],
) -> CryptoResult<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = chacha20poly1305::XNonce::from_slice(nonce);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| CryptoError::DecryptionFailed)
}

/// Encrypt plaintext with XChaCha20-Poly1305, binding `aad` (authenticated, not
/// encrypted): decryption succeeds only with the same `aad`.
pub fn encrypt_with_aad(
    plaintext: &[u8],
    aad: &[u8],
    key: &[u8; KEY_SIZE],
) -> CryptoResult<(Vec<u8>, [u8; NONCE_SIZE])> {
    use chacha20poly1305::aead::Payload;
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, Payload { msg: plaintext, aad })
        .map_err(|_| CryptoError::EncryptionFailed)?;
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    nonce_bytes.copy_from_slice(&nonce);
    Ok((ciphertext, nonce_bytes))
}

/// Decrypt what [`encrypt_with_aad`] sealed; fails if `aad` differs.
pub fn decrypt_with_aad(
    ciphertext: &[u8],
    aad: &[u8],
    nonce: &[u8; NONCE_SIZE],
    key: &[u8; KEY_SIZE],
) -> CryptoResult<Vec<u8>> {
    use chacha20poly1305::aead::Payload;
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce = chacha20poly1305::XNonce::from_slice(nonce);
    cipher
        .decrypt(nonce, Payload { msg: ciphertext, aad })
        .map_err(|_| CryptoError::DecryptionFailed)
}

/// `NONCE_SIZE` fresh random bytes from the OS generator, for an identifier.
pub fn random_bytes() -> [u8; NONCE_SIZE] {
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let mut out = [0u8; NONCE_SIZE];
    out.copy_from_slice(&nonce);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aad_binds_the_ciphertext() {
        let key = [42u8; KEY_SIZE];
        let (ct, nonce) = encrypt_with_aad(b"entry", b"place 1", &key).unwrap();
        assert_eq!(decrypt_with_aad(&ct, b"place 1", &nonce, &key).unwrap(), b"entry");
        assert!(decrypt_with_aad(&ct, b"place 2", &nonce, &key).is_err());
        assert!(decrypt(&ct, &nonce, &key).is_err());
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = [42u8; KEY_SIZE];
        let plaintext = b"hello sovereign GE";
        let (ct, nonce) = encrypt(plaintext, &key).unwrap();
        let recovered = decrypt(&ct, &nonce, &key).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn wrong_key_fails() {
        let key = [42u8; KEY_SIZE];
        let wrong = [99u8; KEY_SIZE];
        let (ct, nonce) = encrypt(b"secret", &key).unwrap();
        assert!(decrypt(&ct, &nonce, &wrong).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = [42u8; KEY_SIZE];
        let (mut ct, nonce) = encrypt(b"secret", &key).unwrap();
        ct[0] ^= 0xff;
        assert!(decrypt(&ct, &nonce, &key).is_err());
    }

    #[test]
    fn empty_plaintext_roundtrip() {
        let key = [7u8; KEY_SIZE];
        let (ct, nonce) = encrypt(b"", &key).unwrap();
        let recovered = decrypt(&ct, &nonce, &key).unwrap();
        assert_eq!(recovered, b"");
    }

    #[test]
    fn large_plaintext_roundtrip() {
        let key = [11u8; KEY_SIZE];
        let plaintext = vec![0xABu8; 1_000_000];
        let (ct, nonce) = encrypt(&plaintext, &key).unwrap();
        let recovered = decrypt(&ct, &nonce, &key).unwrap();
        assert_eq!(recovered, plaintext);
    }
}
