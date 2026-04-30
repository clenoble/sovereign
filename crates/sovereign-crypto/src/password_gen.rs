//! Cryptographically-secure password generator for vault entries.
//!
//! Step 8a of the PII management & dashboard plan. Used by the
//! browser signup-capture flow (8b) when the user clicks "generate
//! password" on a signup form, and exposed to the dashboard's
//! "+ New secret" dialog as a convenience.
//!
//! Uses `rand::rng()` (the thread-local CSPRNG, OS-seeded via
//! `getrandom`) — the same source `master_key.rs`, `kek.rs`, and
//! `document_key.rs` already use for key generation.

use rand::RngExt;
use serde::{Deserialize, Serialize};

const UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.<>?/~";
/// Visually ambiguous characters: 0/O, 1/l/I.
const AMBIGUOUS: &str = "0O1lI";

/// Reasonable default password length. 24 chars from a 90-char set
/// gives ~155 bits of entropy — well above any practical brute-force.
pub const DEFAULT_LENGTH: usize = 24;

/// Knobs the signup form (or vault dialog) exposes to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordPolicy {
    pub length: usize,
    pub include_uppercase: bool,
    pub include_lowercase: bool,
    pub include_digits: bool,
    pub include_symbols: bool,
    /// Drop characters that look alike across fonts (`0O1lI`). Useful
    /// when the password may be transcribed by hand.
    pub exclude_ambiguous: bool,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            length: DEFAULT_LENGTH,
            include_uppercase: true,
            include_lowercase: true,
            include_digits: true,
            include_symbols: true,
            exclude_ambiguous: true,
        }
    }
}

/// Errors from password generation. Both branches are caller bugs
/// (zero length, every charset disabled) — the OS RNG itself
/// shouldn't fail at this layer (a thread-local seeded once succeeds
/// indefinitely).
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PasswordError {
    #[error("password length must be > 0")]
    ZeroLength,
    #[error("character set is empty after the policy was applied")]
    EmptyCharset,
}

/// Generate a password matching `policy`.
///
/// Sampling: `Rng::random_range(0..charset.len())` — `rand` uses
/// rejection sampling internally, so the returned distribution is
/// uniform with no modulo bias.
pub fn generate_password(policy: &PasswordPolicy) -> Result<String, PasswordError> {
    if policy.length == 0 {
        return Err(PasswordError::ZeroLength);
    }
    let charset = build_charset(policy);
    if charset.is_empty() {
        return Err(PasswordError::EmptyCharset);
    }

    let mut rng = rand::rng();
    let mut password = String::with_capacity(policy.length);
    for _ in 0..policy.length {
        let idx = rng.random_range(0..charset.len());
        password.push(charset[idx]);
    }
    Ok(password)
}

/// Build the character pool the generator samples from. Public so the
/// frontend can preview "what will be drawn from" alongside the
/// generated password.
pub fn build_charset(policy: &PasswordPolicy) -> Vec<char> {
    let mut chars: Vec<char> = Vec::with_capacity(96);
    if policy.include_uppercase {
        chars.extend(UPPERCASE.chars());
    }
    if policy.include_lowercase {
        chars.extend(LOWERCASE.chars());
    }
    if policy.include_digits {
        chars.extend(DIGITS.chars());
    }
    if policy.include_symbols {
        chars.extend(SYMBOLS.chars());
    }
    if policy.exclude_ambiguous {
        chars.retain(|c| !AMBIGUOUS.contains(*c));
    }
    chars
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn default_policy_yields_24_char_alphanumeric_symbol() {
        let p = generate_password(&PasswordPolicy::default()).unwrap();
        assert_eq!(p.chars().count(), DEFAULT_LENGTH);
        // Default excludes ambiguous chars.
        assert!(!p.contains('0'));
        assert!(!p.contains('O'));
        assert!(!p.contains('1'));
        assert!(!p.contains('l'));
        assert!(!p.contains('I'));
    }

    #[test]
    fn custom_length_respected() {
        for len in [1, 8, 16, 64, 128] {
            let policy = PasswordPolicy {
                length: len,
                ..PasswordPolicy::default()
            };
            assert_eq!(
                generate_password(&policy).unwrap().chars().count(),
                len
            );
        }
    }

    #[test]
    fn zero_length_errors() {
        let policy = PasswordPolicy {
            length: 0,
            ..PasswordPolicy::default()
        };
        assert_eq!(generate_password(&policy), Err(PasswordError::ZeroLength));
    }

    #[test]
    fn empty_charset_errors() {
        let policy = PasswordPolicy {
            length: 10,
            include_uppercase: false,
            include_lowercase: false,
            include_digits: false,
            include_symbols: false,
            exclude_ambiguous: false,
        };
        assert_eq!(
            generate_password(&policy),
            Err(PasswordError::EmptyCharset)
        );
    }

    #[test]
    fn digits_only_policy() {
        let policy = PasswordPolicy {
            length: 30,
            include_uppercase: false,
            include_lowercase: false,
            include_digits: true,
            include_symbols: false,
            exclude_ambiguous: false,
        };
        let p = generate_password(&policy).unwrap();
        assert!(p.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn lowercase_only_excludes_uppercase_digits_symbols() {
        let policy = PasswordPolicy {
            length: 30,
            include_uppercase: false,
            include_lowercase: true,
            include_digits: false,
            include_symbols: false,
            exclude_ambiguous: false,
        };
        let p = generate_password(&policy).unwrap();
        assert!(p.chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn exclude_ambiguous_drops_them() {
        // Long enough that, without exclusion, ambiguous chars would
        // appear with high probability.
        let policy = PasswordPolicy {
            length: 256,
            ..PasswordPolicy::default()
        };
        let p = generate_password(&policy).unwrap();
        for amb in AMBIGUOUS.chars() {
            assert!(!p.contains(amb), "found ambiguous char {amb:?} in {p:?}");
        }
    }

    #[test]
    fn include_ambiguous_keeps_them_in_charset() {
        let policy = PasswordPolicy {
            exclude_ambiguous: false,
            ..PasswordPolicy::default()
        };
        let charset = build_charset(&policy);
        assert!(charset.contains(&'0'));
        assert!(charset.contains(&'O'));
    }

    #[test]
    fn two_calls_produce_different_passwords() {
        // 24 chars from a 90-char set: collision probability is
        // astronomically low. Two equal results would mean the RNG
        // was reseeded with the same state — which a thread-local
        // OS-seeded CSPRNG never does.
        let p1 = generate_password(&PasswordPolicy::default()).unwrap();
        let p2 = generate_password(&PasswordPolicy::default()).unwrap();
        assert_ne!(p1, p2);
    }

    #[test]
    fn output_is_well_distributed_across_charset() {
        // Generate a single large password and sanity-check that the
        // charset is well-sampled — at least 70% of available chars
        // appear at least once. Catches a degenerate "always the
        // first char" RNG bug.
        let policy = PasswordPolicy {
            length: 1024,
            ..PasswordPolicy::default()
        };
        let charset_len = build_charset(&policy).len();
        let p = generate_password(&policy).unwrap();
        let unique: HashSet<char> = p.chars().collect();
        let coverage = unique.len() as f32 / charset_len as f32;
        assert!(
            coverage > 0.7,
            "charset coverage too low: {} / {} = {coverage:.2}",
            unique.len(),
            charset_len
        );
    }

    #[test]
    fn build_charset_size_default() {
        // 26 + 26 + 10 + 27 = 89 chars, minus 5 ambiguous = 84.
        let charset = build_charset(&PasswordPolicy::default());
        assert_eq!(charset.len(), 84);
    }
}
