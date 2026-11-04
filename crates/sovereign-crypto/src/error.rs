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

    /// An exclusive create (`AuthStore::save_new`) found the path taken.
    #[error("Refusing to replace an existing file: {0}")]
    AlreadyExists(String),

    /// An `AuthStore` constructor was given a duress passphrase equal to the
    /// primary one (v0.0.12 lot 1, P4): such a store would always open the
    /// same persona with that password, whichever it was fixed to. The words
    /// are the desktop wizard's.
    #[error("Duress password must differ from your password")]
    DuressEqualsPrimary,

    #[error("Key not found for document: {0}")]
    KeyNotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Base64 decode error: {0}")]
    Base64(String),

    #[error("Pair payload error: {0}")]
    PairPayload(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Signature verification failed: {0}")]
    VerificationFailed(String),

    #[cfg(feature = "guardian")]
    #[error("Shamir reconstruction failed: need at least {threshold} shards, got {got}")]
    InsufficientShards { threshold: u8, got: usize },

    #[cfg(feature = "guardian")]
    #[error("Recovery error: {0}")]
    RecoveryError(String),
}

pub type CryptoResult<T> = Result<T, CryptoError>;
