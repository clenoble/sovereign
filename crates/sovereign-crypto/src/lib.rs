pub mod account_key;
pub mod aead;
pub mod auth;
pub mod canary;
pub mod device_key;
pub mod pair_payload;
pub mod document_key;
pub mod error;
pub mod fs_private;
pub mod index_key;
pub mod kek;
pub mod key_db;
pub mod keystroke;
pub mod mac;
pub mod master_key;
pub mod password_gen;
pub mod vault;

pub mod migration;

#[cfg(feature = "guardian")]
pub mod guardian;

pub use error::{CryptoError, CryptoResult};

/// A fresh cryptographically-random 32-byte value as lowercase hex (64 chars).
/// Used for shared secrets such as the loopback sidecar token (SIDECAR-002).
pub fn random_hex_32() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
