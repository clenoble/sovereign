use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("AEAD encryption failed")]
    EncryptionFailed,

    #[error("AEAD decryption failed: ciphertext tampered or wrong key")]
    DecryptionFailed,

    #[error("Key derivation failed: {0}")]
    DerivationFailed(String),

    #[error("Invalid key length: expected {expected}, got {got}")]
    InvalidKeyLength { expected: usize, got: usize },

    #[error("Invalid nonce length: expected {expected}, got {got}")]
    InvalidNonceLength { expected: usize, got: usize },

    #[error("Key wrap failed")]
    WrapFailed,

    #[error("Key unwrap failed: wrong wrapping key or tampered data")]
    UnwrapFailed,

    #[error("Key database I/O error: {0}")]
    KeyDbIo(String),

    #[error("Key not found for document: {0}")]
    KeyNotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Base64 decode error: {0}")]
    Base64(String),

    #[cfg(feature = "guardian")]
    #[error("Shamir reconstruction failed: need at least {threshold} shards, got {got}")]
    InsufficientShards { threshold: u8, got: usize },

    #[cfg(feature = "guardian")]
    #[error("Recovery error: {0}")]
    RecoveryError(String),
}

pub type CryptoResult<T> = Result<T, CryptoError>;
