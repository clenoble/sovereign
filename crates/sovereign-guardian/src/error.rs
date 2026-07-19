use thiserror::Error;

#[derive(Debug, Error)]
pub enum GuardianError {
    #[error("identity error: {0}")]
    Identity(String),

    #[error("custody error: {0}")]
    Custody(String),

    #[error("enrollment error: {0}")]
    Enroll(#[from] sovereign_p2p::P2pError),

    #[error("crypto error: {0}")]
    Crypto(#[from] sovereign_crypto::CryptoError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type GuardianResult<T> = Result<T, GuardianError>;
