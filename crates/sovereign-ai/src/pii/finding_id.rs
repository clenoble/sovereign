//! C-SWEEP SW2 (spec ch. 09): one identity for a finding. A finding is keyed
//! by its content's identity, the content's version (a document's normalised
//! hash, C-DOC G28) and its span within its field; the token's record id is
//! derived from that identity, not minted by the store, so that two devices
//! that sweep the same version write the same new version.
//!
//! *Signature by the test author of lot LS(a); the body is the implementer's.*

use sha2::{Digest, Sha256};
use sovereign_docs::TextField;

/// The record id (`pii_record:<key>`, a valid record id of the store) of the
/// finding at `start..end` (byte offsets) in `field` of the content
/// `content_id` at version `version`. The same inputs give the same id on every
/// device and in every run; any input that differs gives another id.
///
/// The key is 128 bits of a SHA-256 over the length-prefixed inputs, written
/// with the letters `a` to `p` only: a token's text holds no digit, so no
/// detection rule (a phone number, an IBAN, an AVS number) can find a value
/// inside a token.
pub fn finding_record_id(content_id: &str, version: &str, field: &TextField, start: usize, end: usize) -> String {
    let mut h = Sha256::new();
    let mut part = |s: &str| {
        h.update((s.len() as u64).to_le_bytes());
        h.update(s.as_bytes());
    };
    part("sovereign.pii-finding/v1");
    part(content_id);
    part(version);
    part(&field_name(field));
    part(&start.to_string());
    part(&end.to_string());
    let digest = h.finalize();
    let mut key = String::with_capacity(32);
    for b in &digest[..16] {
        key.push((b'a' + (b >> 4)) as char);
        key.push((b'a' + (b & 0x0f)) as char);
    }
    format!("pii_record:{key}")
}

/// A field as it enters a finding's identity.
fn field_name(field: &TextField) -> String {
    match field {
        TextField::Title => "title".into(),
        TextField::Text(b) => format!("text:{}", b.to_hex()),
        TextField::Caption(b) => format!("caption:{}", b.to_hex()),
        TextField::Alt(b) => format!("alt:{}", b.to_hex()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_holds_no_digit() {
        let id = finding_record_id("document:x", "v", &TextField::Title, 0, 5);
        let key = id.strip_prefix("pii_record:").unwrap();
        assert_eq!(key.len(), 32);
        assert!(key.chars().all(|c| ('a'..='p').contains(&c)), "{id}");
    }
}
