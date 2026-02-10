use thiserror::Error;

#[derive(Debug, Error)]
pub enum P2pError {
    #[error("Transport error: {0}")]
    Transport(String),

    #[error("Identity derivation failed: {0}")]
    Identity(String),

    #[error("Dial failed: {0}")]
    DialError(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Codec error: {0}")]
    Codec(String),

    #[error("Not connected to peer: {0}")]
    NotConnected(String),

    #[error("Pairing error: {0}")]
    PairingError(String),

    #[error("Sync error: {0}")]
    SyncError(String),

    #[error("Channel closed")]
    ChannelClosed,

    #[error("Crypto error: {0}")]
    Crypto(#[from] sovereign_crypto::CryptoError),
}

pub type P2pResult<T> = Result<T, P2pError>;
