//! Per-entry encryption with tamper-proof hash chain for the session log.
//!
//! Each JSONL line is individually encrypted via XChaCha20-Poly1305 and chained
//! by including the SHA-256 hash of the previous line. This creates a
//! blockchain-like integrity guarantee: any modification, deletion, or
//! reordering of entries breaks the chain.
//!
//! On-disk format per line:
//! ```json
//! {"v":1,"prev":"<hex SHA-256>","nonce":"<base64 24B>","ct":"<base64 ciphertext>"}
//! ```

use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use sha2::{Digest, Sha256};
use sovereign_crypto::aead;

/// Genesis hash — used as `prev` for the very first entry in a chain.
pub const GENESIS_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Encrypted envelope stored as one JSONL line.
#[derive(serde::Serialize, serde::Deserialize)]
struct Envelope {
    v: u8,
    prev: String,
    nonce: String,
    ct: String,
}

/// Encrypt a plaintext JSON entry and return `(encrypted_line, hash_of_encrypted_line)`.
///
/// `prev_hash` is the hex SHA-256 of the previous encrypted line (or [`GENESIS_HASH`]).
pub fn encrypt_entry(
    plaintext_json: &str,
    key: &[u8; 32],
    prev_hash: &str,
) -> Result<(String, String)> {
    let (ciphertext, nonce) = aead::encrypt(plaintext_json.as_bytes(), key)
        .map_err(|e| anyhow::anyhow!("session log encrypt: {e}"))?;

    let envelope = Envelope {
        v: 1,
        prev: prev_hash.to_string(),
        nonce: B64.encode(nonce),
        ct: B64.encode(&ciphertext),
    };

    let line = serde_json::to_string(&envelope)?;
    let hash = sha256_hex(line.as_bytes());
    Ok((line, hash))
}

/// Decrypt an encrypted line and return the plaintext JSON string.
pub fn decrypt_entry(line: &str, key: &[u8; 32]) -> Result<String> {
    let envelope: Envelope =
        serde_json::from_str(line).map_err(|e| anyhow::anyhow!("envelope parse: {e}"))?;

    if envelope.v != 1 {
        bail!("unsupported envelope version: {}", envelope.v);
    }

    let nonce_bytes = B64
        .decode(&envelope.nonce)
        .map_err(|e| anyhow::anyhow!("nonce decode: {e}"))?;
    if nonce_bytes.len() != aead::NONCE_SIZE {
        bail!(
            "invalid nonce length: expected {}, got {}",
            aead::NONCE_SIZE,
            nonce_bytes.len()
        );
    }
    let mut nonce = [0u8; aead::NONCE_SIZE];
    nonce.copy_from_slice(&nonce_bytes);

    let ciphertext = B64
        .decode(&envelope.ct)
        .map_err(|e| anyhow::anyhow!("ciphertext decode: {e}"))?;

    let plaintext = aead::decrypt(&ciphertext, &nonce, key)
        .map_err(|e| anyhow::anyhow!("session log decrypt: {e}"))?;

    String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("utf8: {e}"))
}

/// Check whether a line looks like an encrypted envelope (has `v`, `prev`, `nonce`, `ct` fields).
pub fn is_encrypted_line(line: &str) -> bool {
    // Quick JSON check — avoid full deserialization for the common path
    let trimmed = line.trim();
    trimmed.contains("\"v\":")
        && trimmed.contains("\"prev\":")
        && trimmed.contains("\"nonce\":")
        && trimmed.contains("\"ct\":")
}

/// Verify hash-chain integrity across a slice of raw lines (no decryption needed).
///
/// Returns `Ok(())` if the chain is valid, or an error describing the first break.
pub fn verify_chain(lines: &[String]) -> Result<()> {
    let mut expected_prev = GENESIS_HASH.to_string();

    for (i, line) in lines.iter().enumerate() {
        if !is_encrypted_line(line) {
            // Plaintext lines don't participate in the chain — they predate encryption.
            // Reset expected_prev to the hash of this plaintext line so the next
            // encrypted entry can chain from it.
            expected_prev = sha256_hex(line.as_bytes());
            continue;
        }

        let envelope: Envelope = serde_json::from_str(line)
            .map_err(|e| anyhow::anyhow!("line {i}: envelope parse: {e}"))?;

        if envelope.prev != expected_prev {
            bail!(
                "chain break at line {i}: expected prev={}, got prev={}",
                &expected_prev[..16],
                &envelope.prev[..16.min(envelope.prev.len())]
            );
        }

        expected_prev = sha256_hex(line.as_bytes());
    }

    Ok(())
}

/// Compute hex-encoded SHA-256 of raw bytes.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    // Manual hex encode to avoid pulling in the hex crate
    result.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_KEY: [u8; 32] = [42u8; 32];

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let json = r#"{"ts":"2026-02-28T10:00:00Z","type":"user_input","content":"hello"}"#;
        let (encrypted, _hash) = encrypt_entry(json, &TEST_KEY, GENESIS_HASH).unwrap();
        let decrypted = decrypt_entry(&encrypted, &TEST_KEY).unwrap();
        assert_eq!(decrypted, json);
    }

    #[test]
    fn is_encrypted_detection() {
        let plaintext = r#"{"ts":"2026-02-28T10:00:00Z","type":"user_input","content":"hello"}"#;
        assert!(!is_encrypted_line(plaintext));

        let (encrypted, _) = encrypt_entry(plaintext, &TEST_KEY, GENESIS_HASH).unwrap();
        assert!(is_encrypted_line(&encrypted));
    }

    #[test]
    fn genesis_hash_used_for_first_entry() {
        let json = r#"{"type":"test"}"#;
        let (line, _) = encrypt_entry(json, &TEST_KEY, GENESIS_HASH).unwrap();
        let envelope: Envelope = serde_json::from_str(&line).unwrap();
        assert_eq!(envelope.prev, GENESIS_HASH);
    }

    #[test]
    fn chain_of_three_verifies() {
        let mut lines = Vec::new();
        let mut prev = GENESIS_HASH.to_string();

        for i in 0..3 {
            let json = format!(r#"{{"entry":{i}}}"#);
            let (line, hash) = encrypt_entry(&json, &TEST_KEY, &prev).unwrap();
            lines.push(line);
            prev = hash;
        }

        assert!(verify_chain(&lines).is_ok());
    }

    #[test]
    fn tampered_line_breaks_chain() {
        let mut lines = Vec::new();
        let mut prev = GENESIS_HASH.to_string();

        for i in 0..3 {
            let json = format!(r#"{{"entry":{i}}}"#);
            let (line, hash) = encrypt_entry(&json, &TEST_KEY, &prev).unwrap();
            lines.push(line);
            prev = hash;
        }

        // Tamper with middle line
        lines[1] = lines[1].replace("\"v\":1", "\"v\":1,\"x\":1");
        assert!(verify_chain(&lines).is_err());
    }

    #[test]
    fn deleted_entry_breaks_chain() {
        let mut lines = Vec::new();
        let mut prev = GENESIS_HASH.to_string();

        for i in 0..3 {
            let json = format!(r#"{{"entry":{i}}}"#);
            let (line, hash) = encrypt_entry(&json, &TEST_KEY, &prev).unwrap();
            lines.push(line);
            prev = hash;
        }

        // Remove middle entry
        lines.remove(1);
        assert!(verify_chain(&lines).is_err());
    }

    #[test]
    fn reordered_entries_break_chain() {
        let mut lines = Vec::new();
        let mut prev = GENESIS_HASH.to_string();

        for i in 0..3 {
            let json = format!(r#"{{"entry":{i}}}"#);
            let (line, hash) = encrypt_entry(&json, &TEST_KEY, &prev).unwrap();
            lines.push(line);
            prev = hash;
        }

        // Swap entries 0 and 1
        lines.swap(0, 1);
        assert!(verify_chain(&lines).is_err());
    }

    #[test]
    fn wrong_key_fails_decrypt() {
        let json = r#"{"type":"secret"}"#;
        let (encrypted, _) = encrypt_entry(json, &TEST_KEY, GENESIS_HASH).unwrap();
        let wrong_key = [99u8; 32];
        assert!(decrypt_entry(&encrypted, &wrong_key).is_err());
    }

    #[test]
    fn mixed_plaintext_and_encrypted_chain_verifies() {
        let plaintext = r#"{"ts":"2026-02-28T10:00:00Z","type":"user_input","content":"seed"}"#;
        let prev = sha256_hex(plaintext.as_bytes());

        let (encrypted, _) =
            encrypt_entry(r#"{"type":"real"}"#, &TEST_KEY, &prev).unwrap();

        let lines = vec![plaintext.to_string(), encrypted];
        assert!(verify_chain(&lines).is_ok());
    }
}
