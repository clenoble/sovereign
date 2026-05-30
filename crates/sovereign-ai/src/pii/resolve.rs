//! Resolution API — expand canonical bodies (with `[pii:<record_id>]`
//! tokens) back into displayable forms.
//!
//! Step 5 of the PII management & dashboard plan. Four access levels:
//!
//!   - `Preview`      — type-only label (`[Email]`, `[Phone]`, etc.).
//!                      L1 Observe. No DB writes.
//!   - `MaskedSample` — kind-specific masked form (`a***e@e***e.com`).
//!                      L1 Observe. Decrypts but doesn't bump
//!                      `last_revealed_at`.
//!   - `Reveal`       — full decrypted value. L3 Modify — sets
//!                      `last_revealed_at = now` on every record
//!                      that was successfully decrypted.
//!   - `RawOriginal`  — handled by [`resolve_raw_original`] separately,
//!                      since it operates on a Document/Message's
//!                      `body_raw_encrypted` blob, not on the
//!                      `[pii:<id>]` tokens in `body_canonical`.
//!
//! Errors never abort the whole resolution: a missing record or
//! decrypt failure replaces the token with `[pii:missing]` /
//! `[pii:error]` so the rest of the body still renders.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::vault::EncryptedBlob;
use sovereign_db::schema::PiiKind;
use sovereign_db::traits::GraphDB;

/// What kind of view the caller wants for resolved tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessLevel {
    /// Type-only label, e.g. `[Email]`. L1 Observe — no decryption.
    Preview,
    /// Kind-specific masked form, e.g. `a***e@e***e.com`. L1 Observe.
    MaskedSample,
    /// Full decrypted value. L3 Modify — bumps `last_revealed_at`.
    Reveal,
    /// Handled separately via [`resolve_raw_original`]; included here
    /// so the Tauri command's enum maps cleanly.
    RawOriginal,
}

/// Span of one `[pii:<record_id>]` token in a body.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TokenSpan {
    start: usize,
    end: usize,
    record_id: String,
}

/// Find every `[pii:<record_id>]` token in `body`. Tokens with no
/// closing `]` are skipped (the body is whatever it is — a malformed
/// token shouldn't break rendering of valid ones around it).
fn parse_tokens(body: &str) -> Vec<TokenSpan> {
    const PREFIX: &str = "[pii:";
    let mut out = Vec::new();
    let mut cursor = 0;
    while cursor < body.len() {
        let Some(rel) = body[cursor..].find(PREFIX) else {
            break;
        };
        let start = cursor + rel;
        let after = start + PREFIX.len();
        let Some(rel_end) = body[after..].find(']') else {
            // No closing bracket past this point — bail.
            break;
        };
        let end_idx = after + rel_end;
        let record_id = body[after..end_idx].to_string();
        out.push(TokenSpan {
            start,
            end: end_idx + 1,
            record_id,
        });
        cursor = end_idx + 1;
    }
    out
}

/// Type-only label rendered for `AccessLevel::Preview`.
pub fn preview_label_for_kind(kind: &PiiKind) -> &'static str {
    match kind {
        PiiKind::Email => "[Email]",
        PiiKind::Phone => "[Phone]",
        PiiKind::Ssn => "[SSN]",
        PiiKind::CreditCard => "[Card]",
        PiiKind::Ipv4 => "[IP]",
        PiiKind::Avs => "[AVS]",
        PiiKind::Iban => "[IBAN]",
        PiiKind::Passport => "[Passport]",
        PiiKind::Dob => "[DOB]",
        PiiKind::Address => "[Address]",
        PiiKind::PersonName => "[Name]",
        PiiKind::OrgName => "[Org]",
        PiiKind::Password => "[Password]",
        PiiKind::ApiToken => "[API Token]",
        PiiKind::BankAccount => "[Bank Account]",
        PiiKind::DocumentId => "[Document ID]",
        PiiKind::Note => "[Note]",
        PiiKind::Other => "[PII]",
    }
}

/// Apply a kind-specific masking transform to a plaintext PII value.
/// Length-preserving where it makes sense; redacts everything for the
/// catch-all kinds.
pub fn mask_for_kind(kind: &PiiKind, plaintext: &str) -> String {
    match kind {
        PiiKind::Email => mask_email(plaintext),
        PiiKind::Phone | PiiKind::CreditCard => mask_keep_last_n(plaintext, 4),
        PiiKind::Iban => mask_iban(plaintext),
        PiiKind::Avs => mask_avs(plaintext),
        PiiKind::Ipv4 => mask_ipv4(plaintext),
        PiiKind::Dob => mask_dob(plaintext),
        PiiKind::PersonName => mask_name(plaintext),
        PiiKind::OrgName => mask_org(plaintext),
        PiiKind::Address => "[Address]".to_string(),
        // Passport, SSN, Password, ApiToken, BankAccount, DocumentId,
        // Note, Other: generic first/last + middle asterisks.
        _ => mask_generic(plaintext),
    }
}

fn mask_email(s: &str) -> String {
    if let Some(at_idx) = s.rfind('@') {
        let local = &s[..at_idx];
        let domain = &s[at_idx + 1..];
        format!("{}@{}", mask_inner(local), mask_domain(domain))
    } else {
        mask_generic(s)
    }
}

fn mask_inner(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    match chars.len() {
        0 => String::new(),
        1 | 2 => "*".repeat(chars.len()),
        _ => {
            let first = chars[0];
            let last = chars[chars.len() - 1];
            let middle = "*".repeat(chars.len() - 2);
            format!("{first}{middle}{last}")
        }
    }
}

fn mask_domain(domain: &str) -> String {
    // Keep the TLD visible (helps the user recognize the entity from
    // the masked form: `a***e@e***e.com` vs `…org` is meaningful).
    if let Some(dot) = domain.rfind('.') {
        let core = &domain[..dot];
        let tld = &domain[dot..];
        format!("{}{}", mask_inner(core), tld)
    } else {
        mask_inner(domain)
    }
}

fn mask_keep_last_n(s: &str, n: usize) -> String {
    // Count *digits* from the right, not characters — phone numbers and
    // credit cards may contain spaces, dashes, or dots as separators
    // ("+41 79 555 12 34", "4242-4242-4242-4242"). Keeping last N chars
    // would slice into the digit run mid-group and hide the wrong portion.
    // Walk backwards collecting until we've seen N digits, then mask
    // every digit before that boundary while preserving the separators.
    let chars: Vec<char> = s.chars().collect();
    let mut digits_seen = 0;
    let mut keep_from = chars.len();
    for (i, c) in chars.iter().enumerate().rev() {
        if c.is_ascii_digit() {
            digits_seen += 1;
            if digits_seen == n {
                keep_from = i;
                break;
            }
        }
    }
    if digits_seen < n {
        // Not enough digits — fall back to redacting every digit.
        return chars
            .iter()
            .map(|c| if c.is_ascii_digit() { '*' } else { *c })
            .collect();
    }
    let prefix: String = chars[..keep_from]
        .iter()
        .map(|c| if c.is_ascii_digit() { '*' } else { *c })
        .collect();
    let tail: String = chars[keep_from..].iter().collect();
    format!("{prefix}{tail}")
}

fn mask_iban(s: &str) -> String {
    // Country code (2) + 2 check digits visible; tail digit visible
    // for at-a-glance disambiguation between accounts.
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 6 {
        return mask_generic(s);
    }
    let prefix: String = chars[..4].iter().collect();
    let last = chars[chars.len() - 1];
    let middle = "*".repeat(chars.len() - 5);
    format!("{prefix}{middle}{last}")
}

fn mask_avs(s: &str) -> String {
    // 756.****.****.NN — country prefix + last 2 digits.
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 4 {
        return mask_generic(s);
    }
    let last_two: String = chars[chars.len() - 2..].iter().collect();
    format!("756.****.****.{last_two}")
}

fn mask_ipv4(s: &str) -> String {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() == 4 {
        format!("***.***.***.{}", parts[3])
    } else {
        mask_generic(s)
    }
}

fn mask_dob(s: &str) -> String {
    // Keep the day visible (lowest-information component); mask year
    // and month.
    if let Some(last_dash) = s.rfind('-') {
        format!("****-**-{}", &s[last_dash + 1..])
    } else {
        mask_generic(s)
    }
}

fn mask_name(s: &str) -> String {
    // "Alice Smith" → "A. S."
    let initials: Vec<String> = s
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .map(|c| format!("{c}."))
        .collect();
    if initials.is_empty() {
        mask_generic(s)
    } else {
        initials.join(" ")
    }
}

fn mask_org(s: &str) -> String {
    if let Some(first) = s.split_whitespace().next() {
        format!("{first} ***")
    } else {
        mask_generic(s)
    }
}

fn mask_generic(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    match chars.len() {
        0 => String::new(),
        1 => "*".to_string(),
        2 => "**".to_string(),
        _ => {
            let first = chars[0];
            let last = chars[chars.len() - 1];
            let middle = "*".repeat(chars.len() - 2);
            format!("{first}{middle}{last}")
        }
    }
}

/// Resolve every `[pii:<id>]` token in `body` per `access_level`.
///
/// Side effects: when `access_level == Reveal`, sets
/// `last_revealed_at = now` on every record successfully decrypted.
/// Records that fail to look up are replaced with `[pii:missing]`;
/// records that fail to decrypt with `[pii:error]`. The body's
/// non-token spans are preserved verbatim.
///
/// `RawOriginal` doesn't apply at the token level — use
/// [`resolve_raw_original`] for that. If `RawOriginal` is passed
/// here, tokens are replaced with `[pii:use_raw_original]` as a hint
/// to the caller.
pub async fn resolve_body(
    db: &dyn GraphDB,
    account_key: &AccountKey,
    body: &str,
    access_level: AccessLevel,
) -> String {
    let tokens = parse_tokens(body);
    if tokens.is_empty() {
        return body.to_string();
    }

    let mut out = String::with_capacity(body.len());
    let mut cursor = 0;
    let now = Utc::now();

    for token in &tokens {
        out.push_str(&body[cursor..token.start]);
        let replacement =
            resolve_one(db, account_key, &token.record_id, access_level, now).await;
        out.push_str(&replacement);
        cursor = token.end;
    }
    out.push_str(&body[cursor..]);
    out
}

async fn resolve_one(
    db: &dyn GraphDB,
    account_key: &AccountKey,
    record_id: &str,
    access_level: AccessLevel,
    now: DateTime<Utc>,
) -> String {
    let record = match db.get_pii_record(record_id).await {
        Ok(r) => r,
        Err(_) => return "[pii:missing]".to_string(),
    };

    match access_level {
        AccessLevel::Preview => preview_label_for_kind(&record.kind).to_string(),
        AccessLevel::MaskedSample => {
            let blob = EncryptedBlob::from_pair(
                record.value_encrypted.clone(),
                record.value_nonce.clone(),
            );
            match blob.decrypt_to_string(account_key) {
                Ok(plaintext) => mask_for_kind(&record.kind, &plaintext),
                Err(_) => "[pii:error]".to_string(),
            }
        }
        AccessLevel::Reveal => {
            let blob = EncryptedBlob::from_pair(
                record.value_encrypted.clone(),
                record.value_nonce.clone(),
            );
            match blob.decrypt_to_string(account_key) {
                Ok(plaintext) => {
                    if let Err(e) = db.update_pii_record_revealed_at(record_id, now).await {
                        tracing::warn!(
                            "resolve: update_pii_record_revealed_at({record_id}) failed: {e}"
                        );
                    }
                    plaintext
                }
                Err(_) => "[pii:error]".to_string(),
            }
        }
        AccessLevel::RawOriginal => "[pii:use_raw_original]".to_string(),
    }
}

/// Decrypt a `body_raw_encrypted` + `body_raw_nonce` pair (from
/// `Document` or `Message`) to recover the pre-tokenization original.
/// L3 Modify in the plan; the Tauri command layer enforces the
/// confirmation prompt.
pub fn resolve_raw_original(
    account_key: &AccountKey,
    body_raw_encrypted: &str,
    body_raw_nonce: &str,
) -> anyhow::Result<String> {
    let blob = EncryptedBlob::from_pair(
        body_raw_encrypted.to_string(),
        body_raw_nonce.to_string(),
    );
    blob.decrypt_to_string(account_key)
        .map_err(|e| anyhow::anyhow!("body_raw decrypt failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_crypto::master_key::MasterKey;
    use sovereign_db::mock::MockGraphDB;
    use sovereign_db::schema::{PiiRecord, ReviewState};

    fn test_account_key() -> AccountKey {
        let mk = MasterKey::from_passphrase(b"resolve-test", b"salt").unwrap();
        AccountKey::derive(&mk).unwrap()
    }

    async fn write_record(
        db: &MockGraphDB,
        kind: PiiKind,
        plaintext: &str,
        account_key: &AccountKey,
    ) -> String {
        let blob = EncryptedBlob::encrypt_str(plaintext, account_key).unwrap();
        let record = PiiRecord {
            id: None,
            kind,
            value_encrypted: blob.ciphertext_b64,
            value_nonce: blob.nonce_b64,
            label: None,
            entity_id: None,
            stored_secret: false,
            confidence: 1.0,
            sources: vec![],
            discovered_at: Utc::now(),
            last_revealed_at: None,
            use_count: 0,
            review_state: ReviewState::Confirmed,
            deleted_at: None,
        };
        let created = db.create_pii_record(record).await.unwrap();
        sovereign_db::schema::thing_to_raw(created.id.as_ref().unwrap())
    }

    // --- parse_tokens ---

    #[test]
    fn parse_no_tokens() {
        assert!(parse_tokens("hello world").is_empty());
    }

    #[test]
    fn parse_single_token() {
        let body = "before [pii:a] after";
        let toks = parse_tokens(body);
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].record_id, "a");
        assert_eq!(&body[toks[0].start..toks[0].end], "[pii:a]");
    }

    #[test]
    fn parse_multiple_tokens() {
        let body = "[pii:a] middle [pii:b] end";
        let toks = parse_tokens(body);
        assert_eq!(toks.len(), 2);
        assert_eq!(toks[0].record_id, "a");
        assert_eq!(toks[1].record_id, "b");
    }

    #[test]
    fn parse_id_with_colons() {
        // SurrealDB IDs are "table:key" — record_ids contain at least
        // one colon.
        let body = "[pii:pii_record:abc123]";
        let toks = parse_tokens(body);
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].record_id, "pii_record:abc123");
    }

    #[test]
    fn parse_unclosed_token_skipped() {
        // No closing bracket — the parser bails on the malformed token
        // but doesn't panic. (There happens to be no following token,
        // so the result is empty.)
        let body = "[pii:abc no closing bracket";
        assert!(parse_tokens(body).is_empty());
    }

    #[test]
    fn parse_token_at_start_and_end() {
        let body = "[pii:a]";
        let toks = parse_tokens(body);
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].start, 0);
        assert_eq!(toks[0].end, body.len());
    }

    // --- mask_for_kind ---

    #[test]
    fn mask_email_basic() {
        assert_eq!(mask_for_kind(&PiiKind::Email, "alice@example.com"), "a***e@e*****e.com");
    }

    #[test]
    fn mask_email_short_local() {
        // Single-char local: "a@b.c" → masks the whole local + core
        // part of the domain.
        assert_eq!(mask_for_kind(&PiiKind::Email, "a@b.c"), "*@*.c");
    }

    #[test]
    fn mask_phone_keeps_last_4() {
        let masked = mask_for_kind(&PiiKind::Phone, "+41 79 555 12 34");
        assert!(masked.ends_with("12 34") || masked.ends_with("1234"),
            "got {masked:?}");
    }

    #[test]
    fn mask_iban_keeps_country_and_last() {
        let masked = mask_for_kind(&PiiKind::Iban, "CH9300762011623852957");
        assert!(masked.starts_with("CH93"));
        assert!(masked.ends_with("7"));
        assert!(masked.contains('*'));
    }

    #[test]
    fn mask_avs_format() {
        let masked = mask_for_kind(&PiiKind::Avs, "756.1234.5678.97");
        assert_eq!(masked, "756.****.****.97");
    }

    #[test]
    fn mask_ipv4_keeps_last_octet() {
        assert_eq!(mask_for_kind(&PiiKind::Ipv4, "192.168.1.42"), "***.***.***.42");
    }

    #[test]
    fn mask_dob_keeps_day() {
        assert_eq!(mask_for_kind(&PiiKind::Dob, "1985-03-12"), "****-**-12");
    }

    #[test]
    fn mask_name_initials() {
        assert_eq!(mask_for_kind(&PiiKind::PersonName, "Alice Smith"), "A. S.");
    }

    #[test]
    fn mask_org_first_word() {
        assert_eq!(mask_for_kind(&PiiKind::OrgName, "Acme AG Schweiz"), "Acme ***");
    }

    // --- preview_label_for_kind ---

    #[test]
    fn preview_labels_distinct_per_kind() {
        // Sanity: a few specific labels match the plan's examples.
        assert_eq!(preview_label_for_kind(&PiiKind::Email), "[Email]");
        assert_eq!(preview_label_for_kind(&PiiKind::Phone), "[Phone]");
        assert_eq!(preview_label_for_kind(&PiiKind::Iban), "[IBAN]");
    }

    // --- resolve_body ---

    #[tokio::test]
    async fn resolve_body_no_tokens_passes_through() {
        let dk = test_account_key();
        let db = MockGraphDB::new();
        let body = "no tokens here";
        let out = resolve_body(&db, &dk, body, AccessLevel::Reveal).await;
        assert_eq!(out, body);
    }

    #[tokio::test]
    async fn resolve_body_preview() {
        let dk = test_account_key();
        let db = MockGraphDB::new();
        let id = write_record(&db, PiiKind::Email, "alice@example.com", &dk).await;
        let body = format!("Email me at [pii:{id}] please.");
        let out = resolve_body(&db, &dk, &body, AccessLevel::Preview).await;
        assert_eq!(out, "Email me at [Email] please.");
    }

    #[tokio::test]
    async fn resolve_body_masked_sample() {
        let dk = test_account_key();
        let db = MockGraphDB::new();
        let id = write_record(&db, PiiKind::Email, "alice@example.com", &dk).await;
        let body = format!("Email me at [pii:{id}].");
        let out = resolve_body(&db, &dk, &body, AccessLevel::MaskedSample).await;
        assert!(out.contains("a***e@e*****e.com"), "got {out:?}");
    }

    #[tokio::test]
    async fn resolve_body_reveal_sets_last_revealed_at() {
        let dk = test_account_key();
        let db = MockGraphDB::new();
        let id = write_record(&db, PiiKind::Email, "alice@example.com", &dk).await;

        // Pre-condition: last_revealed_at is None.
        let pre = db.get_pii_record(&id).await.unwrap();
        assert!(pre.last_revealed_at.is_none());

        let body = format!("Email: [pii:{id}]");
        let out = resolve_body(&db, &dk, &body, AccessLevel::Reveal).await;
        assert!(out.contains("alice@example.com"), "got {out:?}");

        // Post-condition: last_revealed_at populated.
        let post = db.get_pii_record(&id).await.unwrap();
        assert!(post.last_revealed_at.is_some());
    }

    #[tokio::test]
    async fn resolve_body_missing_record_yields_placeholder() {
        let dk = test_account_key();
        let db = MockGraphDB::new();
        let body = "[pii:pii_record:does_not_exist]";
        let out = resolve_body(&db, &dk, body, AccessLevel::Reveal).await;
        assert_eq!(out, "[pii:missing]");
    }

    #[tokio::test]
    async fn resolve_body_preserves_surrounding_text_with_unicode() {
        let dk = test_account_key();
        let db = MockGraphDB::new();
        let id = write_record(&db, PiiKind::Phone, "+41 79 555 12 34", &dk).await;
        let body = format!("Téléphone — [pii:{id}] (mobile)");
        let out = resolve_body(&db, &dk, &body, AccessLevel::Preview).await;
        assert_eq!(out, "Téléphone — [Phone] (mobile)");
    }

    #[tokio::test]
    async fn resolve_body_multiple_tokens_independent() {
        let dk = test_account_key();
        let db = MockGraphDB::new();
        let id_email = write_record(&db, PiiKind::Email, "a@b.com", &dk).await;
        let id_phone = write_record(&db, PiiKind::Phone, "+1 555 123 4567", &dk).await;
        let body = format!("[pii:{id_email}] / [pii:{id_phone}]");
        let out = resolve_body(&db, &dk, &body, AccessLevel::Preview).await;
        assert_eq!(out, "[Email] / [Phone]");
    }

    // --- resolve_raw_original ---

    #[test]
    fn resolve_raw_original_round_trip() {
        let dk = test_account_key();
        let blob = EncryptedBlob::encrypt_str("the original text", &dk).unwrap();
        let recovered = resolve_raw_original(&dk, &blob.ciphertext_b64, &blob.nonce_b64).unwrap();
        assert_eq!(recovered, "the original text");
    }

    #[test]
    fn resolve_raw_original_wrong_key_fails() {
        let dk = test_account_key();
        let mk2 = MasterKey::from_passphrase(b"wrong", b"salt").unwrap();
        let dk2 = AccountKey::derive(&mk2).unwrap();
        let blob = EncryptedBlob::encrypt_str("secret", &dk).unwrap();
        assert!(
            resolve_raw_original(&dk2, &blob.ciphertext_b64, &blob.nonce_b64).is_err()
        );
    }
}
