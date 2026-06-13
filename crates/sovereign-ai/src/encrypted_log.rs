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
    let mut seen_encrypted = false;

    for (i, line) in lines.iter().enumerate() {
        if !is_encrypted_line(line) {
            // SESSIONLOG-002: plaintext lines are only legitimate BEFORE
            // encryption was ever enabled on this file (e.g. v0.0.4 seed data
            // preceding the first encrypted entry). Once any encrypted line has
            // appeared, a plaintext line is a forgery: it carries no MAC and the
            // old code reset the chain anchor to `sha256(plaintext)`, letting an
            // attacker with append access inject arbitrary "history" WITHOUT the
            // key and still pass verification. Reject it (fail closed).
            if seen_encrypted {
                bail!(
                    "chain break at line {i}: plaintext line after encryption began \
                     (possible keyless forgery)"
                );
            }
            // Pre-encryption plaintext: anchor the chain to it so the first
            // encrypted entry can chain from the last plaintext line.
            expected_prev = sha256_hex(line.as_bytes());
            continue;
        }

        seen_encrypted = true;
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

// ---------------------------------------------------------------------------
// SESSIONLOG-003: tail-truncation / rollback detection via a MAC'd anchor.
//
// `verify_chain` only proves the lines on disk link together — lopping off the
// tail (or swapping in an older valid copy) leaves a shorter chain that still
// verifies. To catch that we keep a sidecar "anchor" recording a high-water
// mark `(count, head)` MAC'd under the session-log key, so an attacker who can
// rewrite the log can't forge a matching anchor. The anchor is a LOWER BOUND
// (the chain may legitimately have grown past it between the last anchor write
// and a crash), so being AHEAD of the anchor is fine; being BEHIND it — fewer
// lines, or the anchored line replaced — is truncation/rollback.
// ---------------------------------------------------------------------------

/// Domain separator for the anchor MAC (distinct from the commit MAC).
const ANCHOR_MAC_DOMAIN: &[u8] = b"sovereign-sessionlog-anchor:v1";

#[derive(serde::Serialize, serde::Deserialize)]
struct ChainAnchor {
    count: u64,
    head: String,
    mac: String,
}

fn anchor_body(count: u64, head: &str) -> String {
    format!("{count}:{head}")
}

/// Result of reading the anchor sidecar.
pub enum AnchorStatus {
    /// No anchor file — legacy log or first run. Tolerate (can't fail closed
    /// without breaking pre-feature installs), but truncation is undetectable.
    Missing,
    /// Anchor present but its MAC doesn't verify — forged/corrupt. An attack.
    Forged,
    /// Valid anchor: the chain had at least `count` lines, the last of which
    /// hashed to `head`.
    Valid { count: u64, head: String },
}

/// Write the MAC'd chain anchor. `count` is the total line count, `head` is the
/// SHA-256 of the last line (or [`GENESIS_HASH`] for an empty file).
pub fn write_chain_anchor(path: &std::path::Path, key: &[u8; 32], count: u64, head: &str) -> Result<()> {
    let mac = sovereign_crypto::mac::keyed_mac(key, ANCHOR_MAC_DOMAIN, anchor_body(count, head).as_bytes());
    let anchor = ChainAnchor { count, head: head.to_string(), mac };
    let json = serde_json::to_vec(&anchor)?;
    std::fs::write(path, json).map_err(|e| anyhow::anyhow!("anchor write: {e}"))?;
    Ok(())
}

/// Read + MAC-verify the chain anchor.
pub fn read_chain_anchor(path: &std::path::Path, key: &[u8; 32]) -> AnchorStatus {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return AnchorStatus::Missing,
    };
    let anchor: ChainAnchor = match serde_json::from_slice(&data) {
        Ok(a) => a,
        Err(_) => return AnchorStatus::Forged,
    };
    if sovereign_crypto::mac::verify_keyed_mac(
        key,
        ANCHOR_MAC_DOMAIN,
        anchor_body(anchor.count, &anchor.head).as_bytes(),
        &anchor.mac,
    ) {
        AnchorStatus::Valid { count: anchor.count, head: anchor.head }
    } else {
        AnchorStatus::Forged
    }
}

/// Detect truncation/rollback against a valid anchor. `Err` = the log has fewer
/// lines than the anchored high-water mark, or the anchored line is no longer
/// present at its position (rollback / older-copy swap).
pub fn check_no_truncation(raw_lines: &[String], anchor_count: u64, anchor_head: &str) -> Result<()> {
    let actual = raw_lines.len() as u64;
    if actual < anchor_count {
        bail!("session log truncated: {actual} lines on disk < {anchor_count} anchored (high-water mark)");
    }
    if anchor_count == 0 {
        // SESSIONLOG-002: a count==0 anchor records an EMPTY log (minted on
        // rotation / fresh open). It must not be a free pass for a non-empty
        // file — replaying a stale count==0 anchor over a truncated log would
        // otherwise pass. If lines are present, that's a count-rollback.
        if actual > 0 {
            bail!("session log rollback: anchor records an empty log but {actual} line(s) are present");
        }
        return Ok(());
    }
    let anchored = &raw_lines[(anchor_count - 1) as usize];
    let got = sha256_hex(anchored.as_bytes());
    if got != *anchor_head {
        bail!("session log rollback: line {anchor_count} no longer matches the anchored head");
    }
    Ok(())
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
    fn plaintext_after_encryption_is_rejected() {
        // SESSIONLOG-002: a forged plaintext entry appended after encrypted
        // lines must NOT verify — that was the keyless-forgery primitive.
        let mut lines = Vec::new();
        let mut prev = GENESIS_HASH.to_string();
        for i in 0..2 {
            let (line, hash) =
                encrypt_entry(&format!(r#"{{"entry":{i}}}"#), &TEST_KEY, &prev).unwrap();
            lines.push(line);
            prev = hash;
        }
        // Append a plaintext SessionEntry with NO key — the attack.
        lines.push(
            r#"{"ts":"2026-06-13T00:00:00Z","type":"user_input","content":"forged"}"#.to_string(),
        );
        assert!(
            verify_chain(&lines).is_err(),
            "plaintext appended after encryption must be rejected"
        );
    }

    #[test]
    fn plaintext_before_encryption_still_verifies() {
        // Legitimate migration: plaintext seed lines precede the first encrypted
        // entry, which chains from the last plaintext line's hash.
        let p1 = r#"{"type":"seed","n":1}"#.to_string();
        let p2 = r#"{"type":"seed","n":2}"#.to_string();
        let anchor = sha256_hex(p2.as_bytes());
        let (e1, h1) = encrypt_entry(r#"{"entry":0}"#, &TEST_KEY, &anchor).unwrap();
        let (e2, _h2) = encrypt_entry(r#"{"entry":1}"#, &TEST_KEY, &h1).unwrap();
        let lines = vec![p1, p2, e1, e2];
        assert!(
            verify_chain(&lines).is_ok(),
            "a pre-encryption plaintext prefix must still verify"
        );
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

    // --- SESSIONLOG-003: anchor / truncation detection ---

    fn build_chain(n: usize) -> Vec<String> {
        let mut lines = Vec::new();
        let mut prev = GENESIS_HASH.to_string();
        for i in 0..n {
            let (line, hash) = encrypt_entry(&format!(r#"{{"entry":{i}}}"#), &TEST_KEY, &prev).unwrap();
            lines.push(line);
            prev = hash;
        }
        lines
    }

    #[test]
    fn anchor_roundtrips_and_detects_forgery() {
        let dir = std::env::temp_dir().join("sov_sl003_anchor");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("a.anchor");
        let head = sha256_hex(b"some-line");
        write_chain_anchor(&path, &TEST_KEY, 7, &head).unwrap();
        match read_chain_anchor(&path, &TEST_KEY) {
            AnchorStatus::Valid { count, head: h } => {
                assert_eq!(count, 7);
                assert_eq!(h, head);
            }
            _ => panic!("expected valid anchor"),
        }
        // A different key (attacker without the session key) → Forged.
        assert!(matches!(read_chain_anchor(&path, &[7u8; 32]), AnchorStatus::Forged));
        // Missing file → Missing.
        assert!(matches!(read_chain_anchor(&dir.join("none"), &TEST_KEY), AnchorStatus::Missing));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncation_and_rollback_detected_growth_ok() {
        let lines = build_chain(5);
        let head5 = sha256_hex(lines[4].as_bytes());

        // Exact match: 5 lines, anchored at 5 → ok.
        assert!(check_no_truncation(&lines, 5, &head5).is_ok());

        // Tail truncated to 3 lines, anchor still says 5 → truncation.
        assert!(check_no_truncation(&lines[..3].to_vec(), 5, &head5).is_err());

        // Grown to 7 lines, anchor lags at 5 (crash before anchor update) → ok
        // as long as the anchored line is still present at position 5.
        let mut grown = lines.clone();
        let mut prev = sha256_hex(lines[4].as_bytes());
        for i in 5..7 {
            let (line, hash) = encrypt_entry(&format!(r#"{{"entry":{i}}}"#), &TEST_KEY, &prev).unwrap();
            grown.push(line);
            prev = hash;
        }
        assert!(check_no_truncation(&grown, 5, &head5).is_ok());

        // Rollback: same count but the anchored line was replaced → mismatch.
        let other = build_chain(5);
        // (other[4] differs from lines[4] only if content differs; entries are
        // identical here, so force a rollback by anchoring to a bogus head.)
        assert!(check_no_truncation(&other, 5, &sha256_hex(b"different-head")).is_err());
    }

    #[test]
    fn count_zero_anchor_rejects_nonempty_log() {
        // SESSIONLOG-002: count==0 is only valid for an empty file.
        assert!(check_no_truncation(&[], 0, GENESIS_HASH).is_ok());
        // A stale count==0 anchor replayed over a non-empty (truncated) log → Err.
        let lines = build_chain(3);
        assert!(check_no_truncation(&lines, 0, GENESIS_HASH).is_err());
    }
}
